//! Multi-variant enum pattern — error types with several causes.

use thiserror::Error;
use wasm_bindgen_error::WasmError;

#[derive(Debug, Error)]
#[error("network timeout after {ms}ms")]
pub struct TimeoutError {
    ms: u64,
}

#[derive(Debug, Error)]
#[error("permission denied for resource `{resource}`")]
pub struct PermissionError {
    resource: &'static str,
}

/// An enum collecting multiple failure modes under one JS error name.
///
/// JS sees `"ConnectError"` (Wasm prefix stripped).
#[derive(Debug, Error, WasmError)]
pub enum WasmConnectError {
    #[error("timeout: {0}")]
    Timeout(#[from] TimeoutError),

    #[error("permission: {0}")]
    Permission(#[from] PermissionError),

    #[error("connection refused")]
    Refused,
}

#[test]
fn enum_converts_to_jsvalue() {
    fn assert_from<T: Into<wasm_bindgen::JsValue>>() {}
    assert_from::<WasmConnectError>();
}

#[test]
fn each_variant_constructs() {
    let _ = WasmConnectError::from(TimeoutError { ms: 5000 });
    let _ = WasmConnectError::from(PermissionError {
        resource: "/secret",
    });
    let _ = WasmConnectError::Refused;
}
