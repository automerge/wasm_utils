//! Misuse that must not compile, as `compile_fail` doctests.
//!
//! Doctests check the error _code_ rather than rustc's full output, so they
//! don't break when the toolchain changes its wording. The macros' own
//! messages are asserted by unit tests in `wasm_bindgen_trait`.
//!
//! Control: the shared setup compiles when the impl is correct.
//!
//! ```
//! use wasm_bindgen::prelude::*;
//! use wasm_bindgen_trait::{js_trait, wasm_implements};
//!
//! #[js_trait(js_type = JsCalc)]
//! pub trait Calc {
//!     #[wasm_bindgen(js_name = "add")]
//!     fn js_add(&self, a: u32, b: u32) -> u32;
//! }
//!
//! #[wasm_bindgen]
//! pub struct WasmCalc;
//!
//! #[wasm_implements(Calc)]
//! #[wasm_bindgen(js_class = "WasmCalc")]
//! impl WasmCalc {
//!     #[wasm_bindgen(js_name = "add")]
//!     pub fn js_add(&self, a: u32, b: u32) -> u32 { a + b }
//! }
//! ```
//!
//! Exported under the wrong JS name:
//!
//! ```compile_fail,E0080
//! # use wasm_bindgen::prelude::*;
//! # use wasm_bindgen_trait::{js_trait, wasm_implements};
//! # #[js_trait(js_type = JsCalc)]
//! # pub trait Calc {
//! #     #[wasm_bindgen(js_name = "add")]
//! #     fn js_add(&self, a: u32, b: u32) -> u32;
//! # }
//! # #[wasm_bindgen]
//! # pub struct WasmCalc;
//! #[wasm_implements(Calc)]
//! #[wasm_bindgen(js_class = "WasmCalc")]
//! impl WasmCalc {
//!     #[wasm_bindgen(js_name = "plus")]
//!     pub fn js_add(&self, a: u32, b: u32) -> u32 { a + b }
//! }
//! ```
//!
//! Missing the `js_name` entirely (JS would see `js_add`):
//!
//! ```compile_fail,E0080
//! # use wasm_bindgen::prelude::*;
//! # use wasm_bindgen_trait::{js_trait, wasm_implements};
//! # #[js_trait(js_type = JsCalc)]
//! # pub trait Calc {
//! #     #[wasm_bindgen(js_name = "add")]
//! #     fn js_add(&self, a: u32, b: u32) -> u32;
//! # }
//! # #[wasm_bindgen]
//! # pub struct WasmCalc;
//! #[wasm_implements(Calc)]
//! #[wasm_bindgen(js_class = "WasmCalc")]
//! impl WasmCalc {
//!     pub fn js_add(&self, a: u32, b: u32) -> u32 { a + b }
//! }
//! ```
//!
//! Missing a method:
//!
//! ```compile_fail,E0046
//! # use wasm_bindgen::prelude::*;
//! # use wasm_bindgen_trait::{js_trait, wasm_implements};
//! # #[js_trait(js_type = JsCalc)]
//! # pub trait Calc {
//! #     #[wasm_bindgen(js_name = "add")]
//! #     fn js_add(&self, a: u32, b: u32) -> u32;
//! # }
//! # #[wasm_bindgen]
//! # pub struct WasmCalc;
//! #[wasm_implements(Calc)]
//! #[wasm_bindgen(js_class = "WasmCalc")]
//! impl WasmCalc {}
//! ```
//!
//! Wrong return type:
//!
//! ```compile_fail,E0053
//! # use wasm_bindgen::prelude::*;
//! # use wasm_bindgen_trait::{js_trait, wasm_implements};
//! # #[js_trait(js_type = JsCalc)]
//! # pub trait Calc {
//! #     #[wasm_bindgen(js_name = "add")]
//! #     fn js_add(&self, a: u32, b: u32) -> u32;
//! # }
//! # #[wasm_bindgen]
//! # pub struct WasmCalc;
//! #[wasm_implements(Calc)]
//! #[wasm_bindgen(js_class = "WasmCalc")]
//! impl WasmCalc {
//!     #[wasm_bindgen(js_name = "add")]
//!     pub fn js_add(&self, a: u32, b: u32) -> String { String::new() }
//! }
//! ```
//!
//! A default method body on a `#[js_trait]` method:
//!
//! ```compile_fail
//! # use wasm_bindgen::prelude::*;
//! # use wasm_bindgen_trait::js_trait;
//! #[js_trait(js_type = JsGreeter)]
//! pub trait Greeter {
//!     #[wasm_bindgen(js_name = "greet")]
//!     fn js_greet(&self) -> String { String::from("hi") }
//! }
//! ```
