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

/// The generated `From` impl compiles against real `JsValue`.
#[test]
fn from_impl_exists() {
    fn assert_from<T: Into<wasm_bindgen::JsValue>>() {}
    assert_from::<WasmHydrationError>();
}

/// The `?` operator works through the `#[from]` chain.
#[test]
fn question_mark_chain() {
    fn inner() -> Result<(), HydrationError> {
        Err(HydrationError { field: "name" })
    }

    fn outer() -> Result<(), WasmHydrationError> {
        inner()?;
        Ok(())
    }

    assert!(outer().is_err());
}
