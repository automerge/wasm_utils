//! Runtime tests for `#[derive(WasmError)]`.
//!
//! The point of `WasmError` is that a Rust error, once converted to `JsValue`,
//! surfaces in JS as an `Error` whose `.name` is meaningful (so JS consumers can
//! `catch (e) { if (e.name === "FooError") }`) and whose `.message` comes from
//! `Display`. None of that is observable in host/token tests — it requires a
//! real JS `Error` object.

#![allow(clippy::missing_const_for_fn)]

use js_sys::{Error as JsError, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_error::WasmError;
use wasm_bindgen_test::wasm_bindgen_test;

#[derive(Debug, thiserror::Error, WasmError)]
#[error("hydration failed: {0}")]
pub struct WasmHydrationError(String);

#[derive(Debug, thiserror::Error, WasmError)]
#[wasm_error(js_name = "CustomName")]
#[error("override case")]
pub struct WasmOverrideError;

#[derive(Debug, thiserror::Error, WasmError)]
pub enum WasmSyncError {
    #[error("disconnected: {reason}")]
    Disconnected { reason: String },

    #[error("timed out after {0}ms")]
    Timeout(u32),
}

/// Read `.name` / `.message` off whatever `JsValue` we threw. We funnel through
/// `js_sys::Error` (when the value is one) and fall back to `Reflect` so a
/// non-Error value still yields a useful assertion message.
fn name_and_message(value: &JsValue) -> (String, String) {
    if let Some(err) = value.dyn_ref::<JsError>() {
        return (String::from(err.name()), String::from(err.message()));
    }

    let name = Reflect::get(value, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    let message = Reflect::get(value, &JsValue::from_str("message"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    (name, message)
}

#[wasm_bindgen_test]
fn struct_error_strips_wasm_prefix_for_name() {
    let js: JsValue = WasmHydrationError("db down".into()).into();
    let (name, message) = name_and_message(&js);
    assert_eq!(name, "HydrationError");
    assert_eq!(message, "hydration failed: db down");
}

#[wasm_bindgen_test]
fn js_name_override_sets_exact_name() {
    let js: JsValue = WasmOverrideError.into();
    let (name, message) = name_and_message(&js);
    assert_eq!(name, "CustomName");
    assert_eq!(message, "override case");
}

#[wasm_bindgen_test]
fn enum_struct_variant_message() {
    let js: JsValue = WasmSyncError::Disconnected {
        reason: "peer reset".into(),
    }
    .into();
    let (name, message) = name_and_message(&js);
    assert_eq!(name, "SyncError");
    assert_eq!(message, "disconnected: peer reset");
}

#[wasm_bindgen_test]
fn enum_tuple_variant_message() {
    let js: JsValue = WasmSyncError::Timeout(250).into();
    let (name, message) = name_and_message(&js);
    assert_eq!(name, "SyncError");
    assert_eq!(message, "timed out after 250ms");
}

#[wasm_bindgen_test]
fn converted_value_is_a_real_js_error() {
    let js: JsValue = WasmHydrationError("x".into()).into();
    assert!(
        js.dyn_ref::<JsError>().is_some(),
        "WasmError must produce an actual JS Error instance",
    );
}
