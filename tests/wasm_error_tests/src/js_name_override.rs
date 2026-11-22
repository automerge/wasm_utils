//! Explicit `js_name` override — when the default prefix stripping
//! doesn't produce the right JS error name.

use thiserror::Error;
use wasm_bindgen_error::WasmError;

/// The Rust name doesn't have a "Wasm" prefix, and we want a
/// specific JS error name that differs from the Rust type name.
#[derive(Debug, Error, WasmError)]
#[wasm_error(js_name = "StorageFailure")]
#[error("I/O error: {reason}")]
pub struct JsStorageError {
    reason: &'static str,
}
