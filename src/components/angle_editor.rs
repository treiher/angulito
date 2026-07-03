//! The angle editor: a draggable three-point overlay on the selected frame,
//! with zoom and pan, a magnifier loupe, keyboard nudging, and export of the
//! annotated frame as a PNG.

use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    CanvasRenderingContext2d, HtmlAnchorElement, HtmlCanvasElement, HtmlImageElement, Url,
};

use crate::Frame;
use crate::geometry::{
    angle_deg, aspect_correct, label_position, unit_towards, wedge_is_clockwise,
};
use crate::util::{canvas_to_blob, document, window};
use crate::view::View;

// Colors of the measurement overlay. The live overlay takes them from the
// stylesheet and the PNG export draws them onto a canvas, so each one exists
// twice and the two copies have to be changed together.

/// Red, green and blue of the measurement overlay color. Keep in sync with
/// `--overlay` in assets/main.css, which is `#00c8e6`, that is
/// rgb(0, 200, 230).
const OVERLAY_RGB: (u8, u8, u8) = (0, 200, 230);
/// Opacity of the wedge fill over the overlay color. Keep in sync with the
/// `color-mix` percentage of `.angle-wedge` in assets/main.css, which is 30%.
const WEDGE_OPACITY: f64 = 0.3;
/// Dark casing under the lines. Keep in sync with the stroke of
/// `.angle-line-casing` in assets/main.css, which is rgba(0, 0, 0, 0.55).
const LINE_CASING_COLOR: &str = "rgba(0, 0, 0, 0.55)";
/// Halo behind the angle label. Keep in sync with the stroke of
/// `.angle-label` in assets/main.css, which is rgba(0, 0, 0, 0.6).
const LABEL_HALO_COLOR: &str = "rgba(0, 0, 0, 0.6)";
/// Casing around the point dots. Keep in sync with the stroke of
/// `.handle-dot` in assets/main.css, which is #ffffff.
const DOT_CASING_COLOR: &str = "#ffffff";

/// The overlay color as a CSS color string, for the canvas export.
fn overlay_color() -> String {
    let (r, g, b) = OVERLAY_RGB;
    format!("rgb({r}, {g}, {b})")
}

/// The overlay color at the wedge opacity, for the canvas export.
fn overlay_wedge_fill() -> String {
    let (r, g, b) = OVERLAY_RGB;
    format!("rgba({r}, {g}, {b}, {WEDGE_OPACITY})")
}

// Sizes of the measurement overlay, in the coordinate space of the displayed
// frame. The live overlay divides them by the zoom factor so they keep a
// constant on-screen size, and the export multiplies them by the ratio
// between the longer edge of the image and `EXPORT_REFERENCE_EDGE`. Both
// paths have to use the same values, otherwise the saved image stops matching
// the screen.

/// Radius of the filled wedge drawn at the vertex.
const WEDGE_RADIUS: f64 = 40.0;
/// Distance of the angle label from the vertex, along the bisector.
const LABEL_DIST: f64 = 64.0;
/// Horizontal and vertical margins keeping the label inside the frame.
const LABEL_MARGIN: (f64, f64) = (24.0, 14.0);
/// Font size of the angle label.
const LABEL_FONT_SIZE: f64 = 22.0;
/// Width of the dark halo stroked behind the angle label.
const LABEL_HALO_WIDTH: f64 = 4.0;
/// Width of the two angle lines.
const LINE_WIDTH: f64 = 3.0;
/// Width of the dark casing stroked under the angle lines.
const LINE_CASING_WIDTH: f64 = 6.0;
/// Radius of the dot marking a measured point.
const DOT_RADIUS: f64 = 3.5;
/// Width of the casing stroked around a point dot.
const DOT_CASING_WIDTH: f64 = 1.5;

/// Length in pixels of the longer image edge at which the export draws the
/// overlay at the sizes above. Larger images get a proportionally larger
/// overlay, so an exported frame looks the same at any resolution once scaled
/// to a common size. The value is roughly the on-screen size of the frame on
/// a desktop window, which keeps the export close to what the editor shows.
const EXPORT_REFERENCE_EDGE: f64 = 900.0;

/// Diameter of the magnifier loupe in CSS pixels.
const LOUPE_SIZE: f64 = 130.0;
/// Loupe magnification relative to the current on-screen scale.
const LOUPE_MAGNIFICATION: f64 = 2.0;
/// Distance between the dragged point and the near edge of the loupe.
const LOUPE_GAP: f64 = 30.0;

/// Placement of the loupe and of the magnified image inside it, in viewport
/// pixels.
struct Loupe {
    x: f64,
    y: f64,
    img_x: f64,
    img_y: f64,
    img_w: f64,
    img_h: f64,
}

#[component]
pub fn AngleEditor(
    mut frames: Signal<Vec<Frame>>,
    index: usize,
    mut status: Signal<Option<String>>,
) -> Element {
    // The handle being dragged and the id of the pointer dragging it. Moves
    // of other pointers must not affect the point.
    let mut dragging = use_signal(|| None::<(usize, i32)>);
    // The points while a handle is dragged. Writing them into `frames` on
    // every pointer move would re-render all readers of `frames` per move,
    // most notably the frame strip with all its thumbnails. Instead the drag
    // updates only this copy and commits it to the frame once when the drag
    // ends.
    let mut live_points = use_signal(|| None::<[(f64, f64); 3]>);
    let mut view = use_signal(View::default);
    // Layout size of the stage in CSS pixels (before the zoom transform),
    // kept up to date via onresize. The overlay draws in these coordinates.
    let mut overlay_size = use_signal(|| (0.0f64, 0.0f64));
    // Active pointers on the stage background (not on a handle), used for
    // one-finger panning and two-finger pinch zoom.
    let mut pointers = use_signal(Vec::<(i32, (f64, f64))>::new);
    // Mounted elements, used instead of document-wide id lookups.
    let mut viewport_el = use_signal(|| None::<web_sys::Element>);
    let mut overlay_el = use_signal(|| None::<web_sys::Element>);
    let mut img_el = use_signal(|| None::<HtmlImageElement>);
    // The frame the interaction state belongs to.
    let mut editing = use_signal(|| None::<u64>);

    // Borrow instead of cloning the frame list, because this component
    // re-renders on every pointer move of a drag.
    let frames_read = frames.read();
    let Some(frame) = frames_read.get(index) else {
        return rsx! {};
    };
    // Zoom, pan and drag state are per frame. Reset them when another frame
    // is shown, otherwise a pan clamped to the previous image's size could
    // leave the new image outside the viewport.
    if editing() != Some(frame.id) {
        editing.set(Some(frame.id));
        view.set(View::default());
        dragging.set(None);
        live_points.set(None);
        pointers.write().clear();
    }
    let pts = live_points().unwrap_or(frame.points);
    let [a, vertex, c] = aspect_correct(pts, frame.aspect);
    let angle = angle_deg(a, vertex, c);
    let zoom = view().zoom;
    let (pan_x, pan_y) = view().pan;

    // Overlay geometry in stage pixels. Every on-screen size is divided by
    // zoom to stay constant while zooming, the label margins included. They
    // exist to keep the label text from overflowing the frame edge, and that
    // text shrinks with zoom in stage pixels just like everything else, so a
    // fixed margin would inset the label by a further factor of the zoom.
    let size = overlay_size();
    let p = pts.map(|(x, y)| (x * size.0, y * size.1));
    let label = label_position(
        p,
        LABEL_DIST / zoom,
        (LABEL_MARGIN.0 / zoom, LABEL_MARGIN.1 / zoom),
        size,
    );
    let wedge = wedge_path(p, WEDGE_RADIUS / zoom);

    // Magnifier loupe while dragging a handle: a circular window showing the
    // area around the dragged point at twice the current on-screen scale,
    // floating above the point (below it near the top edge).
    let drag_point = dragging().map(|(i, _)| i);
    let loupe = drag_point.filter(|_| size.0 > 0.0).map(|i| {
        let magnification = LOUPE_MAGNIFICATION * zoom;
        let center = (pan_x + zoom * p[i].0, pan_y + zoom * p[i].1);
        let above = center.1 - LOUPE_SIZE - LOUPE_GAP;
        // A dragged point can sit outside the visible area while the image is
        // zoomed and panned, which puts `center` outside the viewport. Both
        // axes are clamped so the loupe stays inside it either way, instead of
        // being cut away by the viewport's `overflow: hidden`.
        let max_y = (size.1 - LOUPE_SIZE).max(0.0);
        Loupe {
            x: (center.0 - LOUPE_SIZE / 2.0).clamp(0.0, (size.0 - LOUPE_SIZE).max(0.0)),
            // Above the point, or below it when there is no room above.
            y: if above >= 0.0 {
                above
            } else {
                center.1 + LOUPE_GAP
            }
            .clamp(0.0, max_y),
            img_x: LOUPE_SIZE / 2.0 - p[i].0 * magnification,
            img_y: LOUPE_SIZE / 2.0 - p[i].1 * magnification,
            img_w: size.0 * magnification,
            img_h: size.1 * magnification,
        }
    });

    let on_pointer_down = move |evt: Event<PointerData>| {
        let web_evt = evt.as_web_event();
        // Only the primary button, or a touch or pen contact, pans. A right
        // click opens the context menu, which swallows the pointerup. The
        // stale entry in `pointers` would then pan on mere hovering.
        if web_evt.button() != 0 {
            return;
        }
        let Some(viewport) = viewport_el() else {
            return;
        };
        let (pos, _) = viewport_pos(&viewport, web_evt.client_x(), web_evt.client_y());
        let _ = viewport.set_pointer_capture(web_evt.pointer_id());
        let mut ptrs = pointers.write();
        if ptrs.len() < 2 {
            ptrs.push((web_evt.pointer_id(), pos));
        }
    };

    let on_pointer_move = move |evt: Event<PointerData>| {
        let web_evt = evt.as_web_event();
        let Some(viewport) = viewport_el() else {
            return;
        };
        let (pos, size) = viewport_pos(&viewport, web_evt.client_x(), web_evt.client_y());

        // Track background pointers even while a handle drag suppresses
        // panning below. A stale stored position would make the view jump
        // by the accumulated distance once the drag ends.
        let old = pointers
            .write()
            .iter_mut()
            .find(|(id, _)| *id == web_evt.pointer_id())
            .map(|(_, p)| std::mem::replace(p, pos));

        if let Some((i, pointer)) = dragging() {
            // A handle is being dragged: move the angle point, but only for
            // the pointer that grabbed the handle. Other touches would make
            // the point jump between fingers. The overlay's bounding rect
            // already reflects the zoom/pan transform, so the normalized
            // coordinates stay correct at any zoom level.
            if web_evt.pointer_id() != pointer {
                return;
            }
            let Some(overlay) = overlay_el() else {
                return;
            };
            let rect = overlay.get_bounding_client_rect();
            if rect.width() <= 0.0 || rect.height() <= 0.0 {
                return;
            }
            let x = ((f64::from(web_evt.client_x()) - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let y = ((f64::from(web_evt.client_y()) - rect.top()) / rect.height()).clamp(0.0, 1.0);
            let current = *live_points.peek();
            if let Some(mut pts) = current {
                pts[i] = (x, y);
                live_points.set(Some(pts));
            }
            return;
        }

        let Some(old) = old else {
            return;
        };
        let ptrs = pointers.read().clone();
        match *ptrs.as_slice() {
            [_] => view.write().pan_by((pos.0 - old.0, pos.1 - old.1), size),
            [p1, p2] => {
                let other = if p1.0 == web_evt.pointer_id() {
                    p2.1
                } else {
                    p1.1
                };
                let old_dist = ((old.0 - other.0).powi(2) + (old.1 - other.1).powi(2)).sqrt();
                let new_dist = ((pos.0 - other.0).powi(2) + (pos.1 - other.1).powi(2)).sqrt();
                if old_dist < 1.0 {
                    return;
                }
                let old_mid = (f64::midpoint(old.0, other.0), f64::midpoint(old.1, other.1));
                let new_mid = (f64::midpoint(pos.0, other.0), f64::midpoint(pos.1, other.1));
                view.write()
                    .pinch(old_mid, new_mid, new_dist / old_dist, size);
            }
            _ => {}
        }
    };

    // Commits keyboard nudges accumulated in `live_points` to the frame, as
    // set up by `onkeydown` on the handles. This is a no-op while a pointer
    // drag is active, whose own commit happens on pointer up.
    let mut commit_nudges = move || {
        if dragging.peek().is_some() {
            return;
        }
        if let Some(pts) = live_points.take()
            && let Some(frame) = frames.write().get_mut(index)
        {
            frame.points = pts;
        }
    };

    let on_pointer_up = move |evt: Event<PointerData>| {
        let id = evt.as_web_event().pointer_id();
        if dragging().is_some_and(|(_, pointer)| pointer == id) {
            dragging.set(None);
            // Commit the dragged points to the frame. Only now do the other
            // readers of `frames` see the change.
            if let Some(pts) = live_points.take()
                && let Some(frame) = frames.write().get_mut(index)
            {
                frame.points = pts;
            }
        }
        pointers.write().retain(|(p, _)| *p != id);
    };

    let on_wheel = move |evt: Event<WheelData>| {
        evt.prevent_default();
        let web_evt = evt.as_web_event();
        let Some(viewport) = viewport_el() else {
            return;
        };
        let (pos, size) = viewport_pos(&viewport, web_evt.client_x(), web_evt.client_y());
        let factor = if web_evt.delta_y() < 0.0 {
            1.2
        } else {
            1.0 / 1.2
        };
        view.write().zoom_at(pos, factor, size);
    };

    let mut zoom_center = move |factor: f64| {
        let Some(viewport) = viewport_el() else {
            return;
        };
        let rect = viewport.get_bounding_client_rect();
        let size = (rect.width(), rect.height());
        view.write()
            .zoom_at((size.0 / 2.0, size.1 / 2.0), factor, size);
    };

    let export = move |_| {
        let Some(img) = img_el() else {
            return;
        };
        // Cloning the frame is cheap (its image data sits behind an object
        // URL) and detaches the export from the `frames` borrow.
        let Some(frame) = frames.read().get(index).cloned() else {
            return;
        };
        // The <img> element is reused when switching frames, where only its
        // src changes, so it may still show the previous frame, or nothing,
        // until the new src has decoded. Exporting then would composite the
        // points onto the wrong image.
        if !img.complete() || img.current_src() != frame.url {
            status.set(Some(
                "The image is still loading. Try again in a moment.".to_string(),
            ));
            return;
        }
        spawn(async move {
            match export_png(&frame, &img).await {
                Ok(()) => status.set(None),
                Err(err) => {
                    web_sys::console::error_1(&err);
                    status.set(Some(
                        "Could not save the image. The browser may not be \
                         able to export a frame of this size."
                            .to_string(),
                    ));
                }
            }
        });
    };

    rsx! {
        section { class: "angle-editor",
            div {
                id: "viewport",
                class: "editor-viewport",
                onmounted: move |evt| {
                    viewport_el.set(Some(evt.as_web_event()));
                },
                onpointerdown: on_pointer_down,
                onpointermove: on_pointer_move,
                onpointerup: on_pointer_up,
                onpointercancel: on_pointer_up,
                onwheel: on_wheel,
                // A long press or a right click would otherwise open the image
                // context menu, which interrupts dragging a handle.
                oncontextmenu: move |evt: Event<MouseData>| evt.prevent_default(),
                div {
                    class: "editor-stage",
                    style: "transform: translate({pan_x}px, {pan_y}px) scale({zoom});",
                    img {
                        id: "editor-img",
                        src: frame.url.clone(),
                        alt: "The frame being measured",
                        draggable: false,
                        // Suppress the long press gesture on the image, which
                        // on Android fires haptic feedback before opening the
                        // image context menu. Dragging and zooming run on
                        // pointer events, which this does not affect. The
                        // handles sit in the overlay above the image and keep
                        // their tap focus and their emulated mouse events.
                        ontouchstart: move |evt: Event<TouchData>| evt.prevent_default(),
                        onmounted: move |evt| {
                            img_el.set(evt.as_web_event().dyn_into().ok());
                        },
                        onresize: move |evt| {
                            if let Ok(box_size) = evt.data().get_border_box_size() {
                                let size = (box_size.width, box_size.height);
                                overlay_size.set(size);
                                // The pan clamp depends on the viewport size.
                                // Apply it again so a window resize cannot
                                // leave the zoomed image offset outside the
                                // viewport.
                                view.write().clamp_pan(size);
                            }
                        },
                    }
                    svg {
                        id: "overlay",
                        onmounted: move |evt| {
                            overlay_el.set(Some(evt.as_web_event()));
                        },
                        if size.0 > 0.0 && size.1 > 0.0 {
                            path { class: "angle-wedge", d: wedge }
                            // Dark casing under the lines keeps them visible
                            // on red or busy backgrounds.
                            for i in [0, 2] {
                                line {
                                    key: "casing-{i}",
                                    class: "angle-line-casing",
                                    stroke_width: LINE_CASING_WIDTH / zoom,
                                    x1: p[1].0,
                                    y1: p[1].1,
                                    x2: p[i].0,
                                    y2: p[i].1,
                                }
                            }
                            for i in [0, 2] {
                                line {
                                    key: "line-{i}",
                                    class: "angle-line",
                                    stroke_width: LINE_WIDTH / zoom,
                                    x1: p[1].0,
                                    y1: p[1].1,
                                    x2: p[i].0,
                                    y2: p[i].1,
                                }
                            }
                            text {
                                class: "angle-label",
                                font_size: LABEL_FONT_SIZE / zoom,
                                stroke_width: LABEL_HALO_WIDTH / zoom,
                                x: label.0,
                                y: label.1,
                                "{angle:.0}°"
                            }
                            // Each handle: a small dot marking the measured
                            // point (also part of the exported image), a ring
                            // as the visible grab affordance (screen only),
                            // and an invisible, finger-sized hit area.
                            for i in 0..3 {
                                g {
                                    key: "{i}",
                                    class: if drag_point == Some(i) {
                                        "handle-group dragging"
                                    } else {
                                        "handle-group"
                                    },
                                    circle {
                                        class: if i == 1 { "handle-ring vertex" } else { "handle-ring" },
                                        stroke_width: 2.5 / zoom,
                                        cx: p[i].0,
                                        cy: p[i].1,
                                        r: 12.0 / zoom,
                                    }
                                    circle {
                                        class: "handle-dot",
                                        stroke_width: DOT_CASING_WIDTH / zoom,
                                        cx: p[i].0,
                                        cy: p[i].1,
                                        r: DOT_RADIUS / zoom,
                                    }
                                    circle {
                                        class: "handle-hit",
                                        cx: p[i].0,
                                        cy: p[i].1,
                                        r: 22.0 / zoom,
                                        // Keyboard access: focus a handle with Tab,
                                        // nudge it with the arrow keys.
                                        tabindex: "0",
                                        role: "button",
                                        "aria-label": ["Move the first end point",
                                                     "Move the vertex",
                                                     "Move the second end point"][i],
                                        onkeydown: move |evt: Event<KeyboardData>| {
                                            let dir = match evt.key() {
                                                Key::ArrowLeft => (-1.0, 0.0),
                                                Key::ArrowRight => (1.0, 0.0),
                                                Key::ArrowUp => (0.0, -1.0),
                                                Key::ArrowDown => (0.0, 1.0),
                                                _ => return,
                                            };
                                            // Keep the arrows from scrolling the page.
                                            evt.prevent_default();
                                            let (w, h) = *overlay_size.peek();
                                            if w <= 0.0 || h <= 0.0 {
                                                return;
                                            }
                                            // One on-screen pixel per press (ten with
                                            // Shift), so the nudge gets finer as the
                                            // user zooms in.
                                            let px = if evt.modifiers().shift() { 10.0 } else { 1.0 }
                                                / view.peek().zoom;
                                            // Like a drag, nudge only the live copy.
                                            // Writing to `frames` on every repeat of a
                                            // held key would re-render all its readers
                                            // per keypress. Committed on key release
                                            // or when the handle loses focus.
                                            let current = (*live_points.peek())
                                                .or_else(|| frames.peek().get(index).map(|f| f.points));
                                            let Some(mut pts) = current else {
                                                return;
                                            };
                                            pts[i].0 = (pts[i].0 + dir.0 * px / w).clamp(0.0, 1.0);
                                            pts[i].1 = (pts[i].1 + dir.1 * px / h).clamp(0.0, 1.0);
                                            live_points.set(Some(pts));
                                        },
                                        onkeyup: move |evt: Event<KeyboardData>| {
                                            if matches!(
                                                evt.key(),
                                                Key::ArrowLeft | Key::ArrowRight
                                                    | Key::ArrowUp | Key::ArrowDown
                                            ) {
                                                commit_nudges();
                                            }
                                        },
                                        // Tabbing or clicking away can end a nudge
                                        // without a key release on this handle.
                                        onblur: move |_| commit_nudges(),
                                        onpointerdown: move |evt: Event<PointerData>| {
                                            evt.prevent_default();
                                            // Keep the stage pan/pinch logic out of it.
                                            evt.stop_propagation();
                                            let web_evt = evt.as_web_event();
                                            // Only the primary button drags. A right
                                            // click's pointerup is swallowed by the
                                            // context menu, leaving the drag stuck.
                                            if web_evt.button() != 0 {
                                                return;
                                            }
                                            // Capture the pointer so the drag survives
                                            // fast movement and leaving the stage.
                                            if let Some(target) = web_evt
                                                .target()
                                                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                                            {
                                                let _ = target.set_pointer_capture(web_evt.pointer_id());
                                                // prevent_default above also suppresses
                                                // the click's default focus. Focus the
                                                // handle explicitly so the arrow keys
                                                // work right after grabbing it.
                                                if let Some(el) = target.dyn_ref::<web_sys::SvgElement>() {
                                                    let _ = el.focus();
                                                }
                                            }
                                            let Some(points) =
                                                frames.peek().get(index).map(|f| f.points)
                                            else {
                                                return;
                                            };
                                            live_points.set(Some(points));
                                            dragging.set(Some((i, web_evt.pointer_id())));
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(loupe) = loupe {
                    div {
                        class: "loupe",
                        style: "left: {loupe.x}px; top: {loupe.y}px; width: {LOUPE_SIZE}px; height: {LOUPE_SIZE}px;",
                        img {
                            // Decorative: a magnified copy of the frame above.
                            alt: "",
                            src: frame.url.clone(),
                            draggable: false,
                            style: "left: {loupe.img_x}px; top: {loupe.img_y}px; width: {loupe.img_w}px; height: {loupe.img_h}px;",
                        }
                        svg { class: "loupe-mark",
                            circle {
                                class: "handle-dot",
                                cx: "50%",
                                cy: "50%",
                                r: DOT_RADIUS,
                                stroke_width: DOT_CASING_WIDTH,
                            }
                        }
                    }
                }
            }
            div { class: "editor-toolbar",
                span { class: "angle-readout", "Angle: {angle:.0}°" }
                div { class: "zoom-controls",
                    // An icon button's accessible name comes from its content,
                    // so "−" and "+" would be all a screen reader reads out.
                    // `title` is only the pointer tooltip.
                    button {
                        "aria-label": "Zoom out",
                        title: "Zoom out",
                        onclick: move |_| zoom_center(1.0 / 1.5),
                        "−"
                    }
                    span { class: "zoom-level", "{(zoom * 100.0).round()}%" }
                    button {
                        "aria-label": "Zoom in",
                        title: "Zoom in",
                        onclick: move |_| zoom_center(1.5),
                        "+"
                    }
                    button {
                        title: "Reset view",
                        disabled: zoom <= View::MIN_ZOOM,
                        onclick: move |_| view.set(View::default()),
                        "Reset"
                    }
                }
                button { class: "primary", onclick: export, "Save image" }
            }
        }
    }
}

/// SVG path of a filled wedge of radius `r` at the vertex, spanning the
/// measured (inner) angle between the two rays.
fn wedge_path(p: [(f64, f64); 3], r: f64) -> String {
    let v = p[1];
    let u1 = unit_towards(v, p[0]);
    let u2 = unit_towards(v, p[2]);
    // The measured angle is always <= 180°, so the arc is a minor arc and the
    // large-arc flag stays 0. The sweep direction follows the orientation of
    // the two rays, so the wedge always covers the measured angle rather than
    // its reflex counterpart.
    let sweep = i32::from(wedge_is_clockwise(p[0], v, p[2]));
    format!(
        "M {:.2} {:.2} L {:.2} {:.2} A {r:.2} {r:.2} 0 0 {sweep} {:.2} {:.2} Z",
        v.0,
        v.1,
        v.0 + u1.0 * r,
        v.1 + u1.1 * r,
        v.0 + u2.0 * r,
        v.1 + u2.1 * r,
    )
}

/// Position of a client coordinate relative to the viewport, plus the
/// viewport's size.
fn viewport_pos(
    viewport: &web_sys::Element,
    client_x: i32,
    client_y: i32,
) -> ((f64, f64), (f64, f64)) {
    let rect = viewport.get_bounding_client_rect();
    (
        (
            f64::from(client_x) - rect.left(),
            f64::from(client_y) - rect.top(),
        ),
        (rect.width(), rect.height()),
    )
}

/// Composites the full-resolution frame with the angle overlay onto a canvas
/// and triggers a PNG download.
///
/// All overlay sizes are the unzoomed on-screen sizes multiplied by the ratio
/// between the longer edge of the image and `EXPORT_REFERENCE_EDGE`, so the
/// overlay covers the same fraction of the image for every frame of a given
/// resolution. Neither the zoom factor nor the size of the browser window
/// changes the saved image.
async fn export_png(frame: &Frame, img: &HtmlImageElement) -> Result<(), JsValue> {
    let doc = document();
    let (width, height) = (
        f64::from(img.natural_width()),
        f64::from(img.natural_height()),
    );
    let ratio = width.max(height) / EXPORT_REFERENCE_EDGE;

    let canvas: HtmlCanvasElement = doc.create_element("canvas")?.dyn_into()?;
    canvas.set_width(img.natural_width());
    canvas.set_height(img.natural_height());
    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?
        .dyn_into()?;
    ctx.draw_image_with_html_image_element_and_dw_and_dh(img, 0.0, 0.0, width, height)?;

    let p = frame.points.map(|(x, y)| (x * width, y * height));

    // Wedge underneath the lines, spanning the measured angle.
    let u1 = unit_towards(p[1], p[0]);
    let u2 = unit_towards(p[1], p[2]);
    ctx.set_fill_style_str(&overlay_wedge_fill());
    ctx.begin_path();
    ctx.move_to(p[1].0, p[1].1);
    ctx.arc_with_anticlockwise(
        p[1].0,
        p[1].1,
        WEDGE_RADIUS * ratio,
        u1.1.atan2(u1.0),
        u2.1.atan2(u2.0),
        !wedge_is_clockwise(p[0], p[1], p[2]),
    )?;
    ctx.close_path();
    ctx.fill();

    // Lines with a dark casing, as in the live overlay. A round join at the
    // vertex avoids the miter spike that narrow angles would produce.
    ctx.set_line_join("round");
    ctx.begin_path();
    ctx.move_to(p[0].0, p[0].1);
    ctx.line_to(p[1].0, p[1].1);
    ctx.line_to(p[2].0, p[2].1);
    ctx.set_stroke_style_str(LINE_CASING_COLOR);
    ctx.set_line_width(LINE_CASING_WIDTH * ratio);
    ctx.stroke();
    ctx.set_stroke_style_str(&overlay_color());
    ctx.set_line_width(LINE_WIDTH * ratio);
    ctx.stroke();

    // Small dots on the measured points, matching the on-screen handle dots
    // (the grab rings are interaction chrome and are not exported).
    ctx.set_fill_style_str(&overlay_color());
    ctx.set_stroke_style_str(DOT_CASING_COLOR);
    ctx.set_line_width(DOT_CASING_WIDTH * ratio);
    for (x, y) in p {
        ctx.begin_path();
        ctx.arc(x, y, DOT_RADIUS * ratio, 0.0, std::f64::consts::TAU)?;
        ctx.fill();
        ctx.stroke();
    }

    let [a, vertex, c] = aspect_correct(frame.points, frame.aspect);
    let angle = angle_deg(a, vertex, c);
    // Same placement rule and styling as the live overlay, meaning placement
    // on the bisector and a dark halo behind the text, stroked first with the
    // fill on top.
    let label = label_position(
        p,
        LABEL_DIST * ratio,
        (LABEL_MARGIN.0 * ratio, LABEL_MARGIN.1 * ratio),
        (width, height),
    );
    let text = format!("{angle:.0}°");
    ctx.set_font(&format!("bold {}px sans-serif", LABEL_FONT_SIZE * ratio));
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    ctx.set_line_join("round");
    ctx.set_stroke_style_str(LABEL_HALO_COLOR);
    ctx.set_line_width(LABEL_HALO_WIDTH * ratio);
    ctx.stroke_text(&text, label.0, label.1)?;
    ctx.set_fill_style_str(&overlay_color());
    ctx.fill_text(&text, label.0, label.1)?;

    // A blob behind an object URL instead of a data URL, which avoids a
    // base64 copy of the whole image and the browser URL-size limit on
    // large frames.
    let blob = canvas_to_blob(&canvas).await?;
    let url = Url::create_object_url_with_blob(&blob)?;
    let anchor: HtmlAnchorElement = doc.create_element("a")?.dyn_into()?;
    anchor.set_href(&url);
    let now = js_sys::Date::new_0();
    anchor.set_download(&format!(
        "angulito_{:04}-{:02}-{:02}_{:02}-{:02}-{:02}_{angle:.0}deg.png",
        now.get_full_year(),
        now.get_month() + 1,
        now.get_date(),
        now.get_hours(),
        now.get_minutes(),
        now.get_seconds(),
    ));
    // Firefox only honors the download attribute for anchors that are in
    // the document. A detached click downloads nothing there.
    let body = doc.body().ok_or_else(|| JsValue::from_str("no body"))?;
    body.append_child(&anchor)?;
    anchor.click();
    anchor.remove();
    // Revoking immediately could cancel the download in some browsers. The
    // URL only needs to outlive the start of the download, after which the
    // in-flight transfer keeps the blob alive on its own.
    let revoke = Closure::once_into_js(move || {
        let _ = Url::revoke_object_url(&url);
    });
    let _ = window()
        .set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 10_000);
    Ok(())
}
