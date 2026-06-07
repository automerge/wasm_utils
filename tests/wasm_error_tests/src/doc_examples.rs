//! Compilable versions of the `wasm_bindgen_error` crate-level doc examples.
//!
//! These live here instead of as doc-tests because `wasm_bindgen_error` is a
//! proc-macro crate. Proc-macro crates can only export macros — their
//! `Cargo.toml` dependencies (`proc-macro2`, `quote`, `syn`) are for the
//! macro implementation, not available to doc-test compilation. Any doc
//! example referencing `wasm_bindgen::JsValue` or `js_sys::Error` would
//! fail with "unresolved module." This integration test crate has those
//! dependencies, so the same code compiles here.
//!
//! The source doc comments keep `ignore` markers so the examples still
//! render in `rustdoc`.

use thiserror::Error;
use wasm_bindgen::prelude::*;
use wasm_bindgen_error::WasmError;

// ── Doc example: "Motivation" (line 14) ─────────────────────────────
// The hand-written boilerplate that WasmError replaces.

#[derive(Debug, Error)]
#[error("manual error")]
pub struct ManualError;

impl From<ManualError> for JsValue {
    fn from(err: ManualError) -> Self {
        let js_err = js_sys::Error::new(&err.to_string());
        js_err.set_name("ManualError");
        js_err.into()
    }
}

#[test]
fn manual_from_impl_compiles() {
    fn assert_into_jsvalue<T: Into<JsValue>>() {}
    assert_into_jsvalue::<ManualError>();
}

// ── Doc example: "Usage" (line 28) ──────────────────────────────────
// The derive macro in action.

#[derive(Debug, Error)]
#[error("hydration failed")]
pub struct HydrationError;

#[derive(Debug, Error, WasmError)]
#[error(transparent)]
pub struct WasmHydrationError(#[from] HydrationError);

#[derive(Debug, Error)]
#[error("io error")]
pub struct IoError;

#[derive(Debug, Error, WasmError)]
#[wasm_error(js_name = "StorageFailure")]
#[error(transparent)]
pub struct WasmIoError(#[from] IoError);

#[test]
fn derive_usage_compiles() {
    fn assert_into_jsvalue<T: Into<JsValue>>() {}
    assert_into_jsvalue::<WasmHydrationError>();
    assert_into_jsvalue::<WasmIoError>();
}

#[test]
fn question_mark_chains() {
    fn inner() -> Result<(), HydrationError> {
        Err(HydrationError)
    }

    fn outer() -> Result<(), WasmHydrationError> {
        inner()?;
        Ok(())
    }

    assert!(outer().is_err());
}
