//! Manual `Error` + `Display` impl — demonstrates that `thiserror`
//! is not required.

use core::fmt;
use wasm_bindgen_error::WasmError;

/// A simple error with hand-written trait impls.
#[derive(Debug, WasmError)]
pub struct WasmManualError {
    code: u32,
}

impl fmt::Display for WasmManualError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "manual error (code {})", self.code)
    }
}

impl core::error::Error for WasmManualError {}

#[test]
fn manual_impl_compiles() {
    fn assert_from<T: Into<wasm_bindgen::JsValue>>() {}
    assert_from::<WasmManualError>();
}
