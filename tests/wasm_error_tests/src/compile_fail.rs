//! Misuse that must not compile, as `compile_fail` doctests (error codes
//! only; the macro's messages are asserted by `wasm_bindgen_error` unit tests).
//!
//! Control:
//!
//! ```
//! #[derive(Debug, thiserror::Error, wasm_bindgen_error::WasmError)]
//! #[error("boom")]
//! #[wasm_error(js_name = "Boom")]
//! pub struct WasmBoomError;
//! ```
//!
//! The type doesn't implement `Error`:
//!
//! ```compile_fail,E0277
//! #[derive(Debug, wasm_bindgen_error::WasmError)]
//! pub struct WasmNotAnError;
//! ```
//!
//! `js_name` given twice:
//!
//! ```compile_fail
//! #[derive(Debug, thiserror::Error, wasm_bindgen_error::WasmError)]
//! #[error("boom")]
//! #[wasm_error(js_name = "A")]
//! #[wasm_error(js_name = "B")]
//! pub struct WasmBoomError;
//! ```
//!
//! `#[wasm_error]` on a variant:
//!
//! ```compile_fail
//! #[derive(Debug, thiserror::Error, wasm_bindgen_error::WasmError)]
//! pub enum WasmNetError {
//!     #[error("timeout")]
//!     #[wasm_error(js_name = "TimeoutError")]
//!     Timeout,
//! }
//! ```
