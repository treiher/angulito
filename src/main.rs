//! App shell: the shared [`Frame`] state, the top-level layout wiring the
//! components together, and the frame thumbnail strip.

// Nothing in this crate needs unsafe, so let the compiler keep it that way.
#![forbid(unsafe_code)]

mod components;
mod geometry;
mod util;
mod view;

use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;

use components::{AngleEditor, FileLoader, FrameSelector};

/// A still image on which an angle is measured.
#[derive(Clone, PartialEq)]
pub struct Frame {
    /// Identifies the frame across insertions and deletions, where positions
    /// shift (list keys, per-frame editor state).
    pub id: u64,
    /// Object URL of the frame's image, either a loaded file or a captured
    /// video frame. It is revoked when the frame is deleted.
    pub url: String,
    /// Width divided by height of the underlying image.
    pub aspect: f64,
    /// The three angle points (end, vertex, end), normalized to 0..=1
    /// in both axes so they are independent of the displayed size.
    pub points: [(f64, f64); 3],
}

impl Frame {
    pub fn new(url: String, aspect: f64) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            url,
            aspect,
            points: [(0.25, 0.75), (0.5, 0.35), (0.75, 0.75)],
        }
    }
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    // The loaded video file, kept as an object URL into the in-memory file.
    // Still images skip this and become frames directly.
    let video = use_signal(|| None::<String>);
    let mut status = use_signal(|| None::<String>);
    let frames = use_signal(Vec::<Frame>::new);
    let selected = use_signal(|| None::<usize>);

    rsx! {
        document::Stylesheet { href: asset!("/assets/main.css") }
        document::Link {
            rel: "icon",
            r#type: "image/svg+xml",
            href: asset!("/assets/favicon.svg"),
        }
        header {
            h1 { "Angulito" }
            p { "Measure angles in images and video frames. All in your browser." }
        }
        main {
            FileLoader { video, frames, selected, status }
            if let Some(msg) = status() {
                p { class: "status-error", role: "alert",
                    span { "{msg}" }
                    button {
                        class: "status-dismiss",
                        "aria-label": "Dismiss message",
                        title: "Dismiss",
                        onclick: move |_| status.set(None),
                        "×"
                    }
                }
            }
            FrameSelector { video, frames, selected, status }
            if !frames.read().is_empty() {
                FrameStrip { frames, selected }
            }
            if let Some(index) = selected() {
                AngleEditor { frames, index, status }
            }
        }
        footer {
            // Offer of corresponding source, as required by AGPL-3.0 §13.
            a { href: "https://github.com/treiher/angulito", "Source" }
            " · AGPL-3.0"
        }
    }
}

#[component]
fn FrameStrip(mut frames: Signal<Vec<Frame>>, mut selected: Signal<Option<usize>>) -> Element {
    let mut delete = move |i: usize| {
        // The index comes from a render-time closure and could be stale if
        // events ever outrun a re-render. An out of bounds index would abort
        // the app.
        if i >= frames.read().len() {
            return;
        }
        let frame = frames.write().remove(i);
        // Loaded files and captured video frames alike hold an object URL.
        // Release it so the underlying blob can be freed.
        let _ = web_sys::Url::revoke_object_url(&frame.url);
        let len = frames.read().len();
        let new_selected = match selected.peek().as_ref() {
            None => None,
            Some(&s) if s > i => Some(s - 1),
            Some(&s) if s == i => (len > 0).then(|| i.min(len - 1)),
            Some(&s) => Some(s),
        };
        selected.set(new_selected);
    };

    rsx! {
        section { class: "frame-strip",
            for (i, frame) in frames.read().iter().enumerate() {
                div {
                    key: "{frame.id}",
                    class: if selected() == Some(i) { "thumb selected" } else { "thumb" },
                    // A button rather than a bare image, so the frame can be
                    // selected with the keyboard. The image is decorative. The
                    // button's label names the frame, and the selected state
                    // is only conveyed by a border otherwise.
                    button {
                        class: "thumb-select",
                        "aria-label": "Select frame {i + 1}",
                        "aria-pressed": "{selected() == Some(i)}",
                        onclick: move |_| selected.set(Some(i)),
                        img { src: frame.url.clone(), alt: "" }
                    }
                    button {
                        class: "thumb-delete",
                        // An icon button's accessible name comes from its
                        // content, so "×" would be all a screen reader reads
                        // out. `title` is only the pointer tooltip.
                        "aria-label": "Delete frame {i + 1}",
                        title: "Delete frame",
                        onclick: move |_| delete(i),
                        "×"
                    }
                }
            }
        }
    }
}
