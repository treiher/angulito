//! Video playback and frame capture: scrub or single-step through the
//! loaded video and capture the current picture as a frame.

use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlVideoElement, Url};

use crate::Frame;
use crate::util::{canvas_to_blob, document};

/// Assumed frame duration for stepping until the actual one has been
/// measured from playback. 30 fps is a good middle ground.
const FALLBACK_FRAME_STEP: f64 = 1.0 / 30.0;
/// Number of frame-to-frame intervals to measure before the stepping
/// interval is derived from their median.
const FRAME_STEP_SAMPLES: usize = 10;

#[component]
pub fn FrameSelector(
    mut video: Signal<Option<String>>,
    mut frames: Signal<Vec<Frame>>,
    mut selected: Signal<Option<usize>>,
    mut status: Signal<Option<String>>,
) -> Element {
    let mut time = use_signal(|| 0.0f64);
    // Frame duration used for stepping, measured per video (see
    // `estimate_frame_step`).
    let mut frame_step = use_signal(|| FALLBACK_FRAME_STEP);
    // The mounted <video> element. It is reused when another video is loaded,
    // so the handle stays valid for the lifetime of the section.
    let mut video_el = use_signal(|| None::<HtmlVideoElement>);
    // The URL currently shown. Per-video state such as the time readout and
    // the frame step resets when the video is replaced rather than carrying
    // over, and the frame step estimation loop runs only as long as this
    // still names the video it was started for.
    let mut shown = use_signal(|| None::<String>);

    let Some(url) = video() else {
        return rsx! {};
    };
    if shown() != Some(url.clone()) {
        shown.set(Some(url.clone()));
        time.set(0.0);
        frame_step.set(FALLBACK_FRAME_STEP);
        // On the very first render the element is not mounted yet. The
        // estimation is then started by `onmounted` below instead.
        if let Some(video) = video_el.peek().clone() {
            estimate_frame_step(video, url.clone(), shown, frame_step, None, Vec::new());
        }
    }

    // Closing the video keeps already captured frames. They hold their own
    // object URLs into independent PNG blobs.
    let close = move |_| {
        if let Some(url) = video.peek().as_ref() {
            let _ = Url::revoke_object_url(url);
        }
        // A lingering status message (e.g. an undecodable codec) concerns
        // the video and is obsolete once the player is closed.
        status.set(None);
        video.set(None);
        // The <video> element unmounts together with the section, so the handle
        // would keep pointing at a removed element. The next video loaded would
        // then start its frame step estimation against that one instead of the
        // element that actually shows it.
        video_el.set(None);
        // Ends the frame step estimation loop for this video. Without it the
        // loop would only stop because the callback no longer fires on the
        // detached element, which is an accident rather than a guarantee.
        shown.set(None);
    };

    let capture = move |_| {
        spawn(async move {
            let frame = match video_el.peek().clone() {
                Some(video) => capture_current_frame(&video).await,
                None => None,
            };
            let Some(frame) = frame else {
                status.set(Some(
                    "Could not capture a frame. Wait until the video is loaded, \
                     then try again."
                        .to_string(),
                ));
                return;
            };
            status.set(None);
            frames.write().push(frame);
            selected.set(Some(frames.read().len() - 1));
        });
    };

    let step = move |delta: f64| {
        let Some(video) = video_el() else {
            return;
        };
        let duration = video.duration();
        if duration.is_nan() {
            return;
        }
        let _ = video.pause();
        video.set_current_time((video.current_time() + delta).clamp(0.0, duration));
    };

    let mount_url = url.clone();
    rsx! {
        section { class: "frame-selector",
            button {
                class: "video-close",
                // An icon button's accessible name comes from its content, so
                // "×" would be all a screen reader reads out. `title` is only
                // the pointer tooltip.
                "aria-label": "Close video",
                title: "Close video",
                onclick: close,
                "×"
            }
            video {
                id: "video-el",
                src: url,
                controls: true,
                muted: true,
                "playsinline": true,
                onmounted: move |evt| {
                    let el: Option<HtmlVideoElement> = evt.as_web_event().dyn_into().ok();
                    if let Some(video) = el.clone() {
                        estimate_frame_step(
                            video,
                            mount_url.clone(),
                            shown,
                            frame_step,
                            None,
                            Vec::new(),
                        );
                    }
                    video_el.set(el);
                },
                // The MIME type check in the file loader cannot tell whether
                // the codec is actually decodable. Without this the user
                // would just see a dead player.
                onerror: move |_| {
                    status.set(Some(
                        "Could not play the video. The browser may not \
                         support this video format."
                            .to_string(),
                    ));
                },
                ontimeupdate: move |_| {
                    if let Some(video) = video_el() {
                        time.set(video.current_time());
                    }
                },
            }
            div { class: "video-controls",
                div { class: "seek-controls",
                    button { title: "Back 1 second", onclick: move |_| step(-1.0), "−1 s" }
                    button { title: "Previous frame", onclick: move |_| step(-frame_step()), "−1 fr" }
                    span { class: "video-time", "{time():.2} s" }
                    button { title: "Next frame", onclick: move |_| step(frame_step()), "+1 fr" }
                    button { title: "Forward 1 second", onclick: move |_| step(1.0), "+1 s" }
                }
                button { class: "primary", onclick: capture, "Capture frame" }
            }
        }
    }
}

/// Draws the video's current frame onto an off-screen canvas and returns it
/// as an object URL into a PNG blob, wrapped in a new [`Frame`].
///
/// An object URL instead of a data URL, because frames are passed around as
/// their URL strings, and a multi-megabyte base64 string would make every
/// clone and DOM diff expensive.
async fn capture_current_frame(video: &HtmlVideoElement) -> Option<Frame> {
    let doc = document();
    let (w, h) = (video.video_width(), video.video_height());
    if w == 0 || h == 0 {
        return None;
    }
    let canvas: HtmlCanvasElement = doc.create_element("canvas").ok()?.dyn_into().ok()?;
    canvas.set_width(w);
    canvas.set_height(h);
    let ctx: CanvasRenderingContext2d = canvas.get_context("2d").ok()??.dyn_into().ok()?;
    ctx.draw_image_with_html_video_element(video, 0.0, 0.0)
        .ok()?;
    let blob = canvas_to_blob(&canvas).await.ok()?;
    let url = Url::create_object_url_with_blob(&blob).ok()?;
    Some(Frame::new(url, f64::from(w) / f64::from(h)))
}

/// Measures the video's frame duration from playback and stores it in
/// `frame_step`.
///
/// `requestVideoFrameCallback` reports the media time of each presented
/// frame. The deltas between consecutive callbacks during continuous
/// playback are frame durations. Once enough samples are collected, their
/// median becomes the stepping interval and the loop ends. The loop also
/// ends when `shown` no longer holds `url`, which is the case once another
/// video is loaded or the player is closed. Browsers without the API keep
/// the fallback interval.
fn estimate_frame_step(
    video: HtmlVideoElement,
    url: String,
    shown: Signal<Option<String>>,
    mut frame_step: Signal<f64>,
    last_media_time: Option<f64>,
    mut samples: Vec<f64>,
) {
    // A fresh one-shot closure per frame instead of a self-referential
    // recurring one. The latter would have to keep itself alive through a
    // reference cycle, which leaks.
    let target = video.clone();
    let callback = Closure::once_into_js(move |_now: f64, metadata: JsValue| {
        if shown.peek().as_deref() != Some(url.as_str()) {
            return;
        }
        let media_time = js_sys::Reflect::get(&metadata, &JsValue::from_str("mediaTime"))
            .ok()
            .and_then(|t| t.as_f64());
        // Only intervals between frames presented during continuous playback
        // are frame durations. Scrubbing while paused also fires callbacks,
        // but with arbitrary deltas. The range filter of 8 to 240 fps drops
        // the odd jump from a seek during playback.
        let last = match media_time {
            Some(t) if !video.paused() => {
                if let Some(last) = last_media_time {
                    let delta = t - last;
                    if (1.0 / 240.0..=1.0 / 8.0).contains(&delta) {
                        samples.push(delta);
                    }
                }
                Some(t)
            }
            _ => None,
        };
        if samples.len() >= FRAME_STEP_SAMPLES {
            samples.sort_by(f64::total_cmp);
            frame_step.set(samples[samples.len() / 2]);
            return;
        }
        estimate_frame_step(video, url, shown, frame_step, last, samples);
    });
    request_video_frame_callback(&target, callback.unchecked_ref());
}

/// Calls `requestVideoFrameCallback` via reflection, as web-sys exposes it
/// only behind its unstable API flag. A no-op where the API is missing.
fn request_video_frame_callback(video: &HtmlVideoElement, callback: &js_sys::Function) {
    let Some(func) = js_sys::Reflect::get(video, &JsValue::from_str("requestVideoFrameCallback"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
    else {
        return;
    };
    let _ = func.call1(video, callback);
}
