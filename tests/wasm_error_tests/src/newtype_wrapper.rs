//! Newtype wrapper pattern — the most common usage.
//!
//! Wraps a domain error in a Wasm-friendly struct and derives
//! `WasmError` to get `From<Self> for JsValue`.

use thiserror::Error;
use wasm_bindgen_error::WasmError;

/// A domain error from an inner library.
#[derive(Debug, Error)]
#[error("hydration failed: missing field `{field}`")]
pub struct HydrationError {
    field: &'static str,
}

/// Wasm wrapper — strips "Wasm" prefix, so JS sees "HydrationError".
#[derive(Debug, Error, WasmError)]
#[error(transparent)]
pub struct WasmHydrationError(#[from] HydrationError);
