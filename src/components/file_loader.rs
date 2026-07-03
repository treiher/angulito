//! File input: detects whether the selected files are images or a video,
//! turns images directly into frames, and hands a video to the player.

use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlImageElement, HtmlInputElement, Url};

use crate::Frame;

#[component]
pub fn FileLoader(
    mut video: Signal<Option<String>>,
    mut frames: Signal<Vec<Frame>>,
    mut selected: Signal<Option<usize>>,
    mut status: Signal<Option<String>>,
) -> Element {
    let on_change = move |evt: Event<FormData>| {
        let Some(input) = evt
            .as_web_event()
            .target()
            .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
        else {
            return;
        };
        let files: Vec<web_sys::File> = input
            .files()
            .map(|files| (0..files.length()).filter_map(|i| files.get(i)).collect())
            .unwrap_or_default();
        // Allow selecting the same files again later.
        input.set_value("");
        if files.is_empty() {
            return;
        }
        status.set(None);

        let mut errors = Vec::new();
        let mut images = Vec::new();
        let mut video_count = 0;
        for file in files {
            let file_type = file.type_();
            if !file_type.starts_with("image") && !file_type.starts_with("video") {
                errors.push(format!(
                    "\"{}\" is not an image or video file.",
                    file.name()
                ));
                continue;
            }
            let Ok(url) = Url::create_object_url_with_blob(&file) else {
                errors.push(format!("Failed to open \"{}\".", file.name()));
                continue;
            };
            if file_type.starts_with("video") {
                // Only one video can be open at a time, so the first of the
                // selection wins. A previously opened video is replaced.
                // Captured frames hold their own object URLs into independent
                // blobs and stay around.
                video_count += 1;
                if video_count > 1 {
                    let _ = Url::revoke_object_url(&url);
                    continue;
                }
                if let Some(old) = video.peek().as_ref() {
                    let _ = Url::revoke_object_url(old);
                }
                video.set(Some(url));
            } else {
                images.push((file.name(), url));
            }
        }
        if video_count > 1 {
            errors.push(format!(
                "Only one video can be open at a time. The first of the \
                 {video_count} selected videos was opened and the rest were \
                 discarded."
            ));
        }

        // Each image becomes a frame of its own. Decode them in order to
        // learn their aspect ratios, which the angle math depends on.
        spawn(async move {
            for (name, url) in images {
                // A creation failure must not panic, because a panic in WASM
                // kills the whole app until a reload.
                let Ok(img) = HtmlImageElement::new() else {
                    let _ = Url::revoke_object_url(&url);
                    errors.push(format!("Failed to open \"{name}\"."));
                    continue;
                };
                img.set_src(&url);
                if JsFuture::from(img.decode()).await.is_ok() && img.natural_height() > 0 {
                    let aspect = f64::from(img.natural_width()) / f64::from(img.natural_height());
                    frames.write().push(Frame::new(url, aspect));
                    selected.set(Some(frames.read().len() - 1));
                } else {
                    let _ = Url::revoke_object_url(&url);
                    errors.push(format!(
                        "Could not decode \"{name}\". The browser may not \
                         support this image format. Converting it to PNG or \
                         JPEG usually helps."
                    ));
                }
            }
            if !errors.is_empty() {
                status.set(Some(errors.join(" ")));
            }
        });
    };

    rsx! {
        section { class: "file-loader",
            // The input comes first so that the stylesheet can put its focus
            // ring on the label, which is what the user actually sees.
            input {
                id: "file-input",
                r#type: "file",
                accept: "image/*,video/*",
                multiple: true,
                onchange: on_change,
            }
            label { class: "file-button", r#for: "file-input", "Load images or video" }
        }
    }
}
