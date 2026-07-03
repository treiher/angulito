//! DOM and canvas helpers shared across components.

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, HtmlCanvasElement};

pub fn window() -> web_sys::Window {
    web_sys::window().expect("no window")
}

pub fn document() -> web_sys::Document {
    window().document().expect("no document")
}

/// Encodes the canvas as a PNG [`Blob`], wrapping the callback-based
/// `canvas.toBlob` in a `Promise`.
///
/// Unlike `canvas.toDataURL`, this neither doubles the peak memory with a
/// base64 copy nor runs into browser URL-size limits on large images.
pub async fn canvas_to_blob(canvas: &HtmlCanvasElement) -> Result<Blob, JsValue> {
    // The executor closure is `FnMut` but called exactly once, synchronously.
    // The `Option` dance moves the canvas into it.
    let mut canvas = Some(canvas.clone());
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let canvas = canvas.take().expect("executor ran twice");
        let callback = Closure::once_into_js(move |blob: JsValue| {
            let _ = resolve.call1(&JsValue::NULL, &blob);
        });
        if let Err(err) = canvas.to_blob(callback.unchecked_ref()) {
            // On e.g. a SecurityError from a tainted canvas the callback
            // never runs, so fail the promise instead of hanging forever.
            let _ = reject.call1(&JsValue::NULL, &err);
        }
    });
    JsFuture::from(promise)
        .await?
        .dyn_into()
        // Per spec the callback receives null when encoding fails.
        .map_err(|_| JsValue::from_str("canvas.toBlob returned no data"))
}
