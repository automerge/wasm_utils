//! Generic error types with `where` clauses.
//!
//! The derive appends its own `Self: Error` bound to the user's where clause.
//! These compile only if that merge produces valid Rust in every shape.

use core::fmt;

use thiserror::Error;
use wasm_bindgen_error::WasmError;

#[derive(Debug, Error, WasmError)]
#[error("bounded: {0:?}")]
pub struct WasmBoundedError<T>(T)
where
    T: fmt::Debug;

#[rustfmt::skip] // keep the trailing comma under test
#[derive(Debug, Error, WasmError)]
#[error("trailing: {0:?}")]
pub struct WasmTrailingCommaError<T>(T)
where
    T: fmt::Debug;

#[derive(Debug, Error, WasmError)]
#[error("named: {inner:?}")]
pub struct WasmNamedBoundedError<T>
where
    T: fmt::Debug + Clone,
{
    inner: T,
}

#[test]
fn where_clause_types_convert_to_js_value() {
    fn assert_from<T: Into<wasm_bindgen::JsValue>>() {}
    assert_from::<WasmBoundedError<u8>>();
    assert_from::<WasmTrailingCommaError<u8>>();
    assert_from::<WasmNamedBoundedError<u8>>();
}
