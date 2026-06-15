//! TypeScript-acceptance fixture.
//!
//! This crate exercises every macro that emits TypeScript (`#[wasm_refgen]`'s
//! `typescript_type`, `#[js_trait]`'s `typescript_custom_section` interface,
//! exported structs, and `WasmError`). It is compiled with `wasm-pack build`,
//! and the resulting `.d.ts` is consumed by `ts/consumer.ts` and checked with
//! `tsc --noEmit`. If the generated types are malformed or the interface shapes
//! drift, `tsc` fails — catching what Rust-side tests cannot.

#![allow(
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::unused_self,
    missing_docs
)]

use wasm_bindgen::prelude::*;
use wasm_bindgen_error::WasmError;
use wasm_bindgen_trait::js_trait;
use wasm_refgen::wasm_refgen;

#[derive(Clone)]
#[wasm_bindgen(js_name = Counter)]
pub struct WasmCounter {
    value: u32,
}

#[wasm_refgen(js_ref = JsCounter)]
#[wasm_bindgen(js_class = "Counter")]
impl WasmCounter {
    #[wasm_bindgen(constructor)]
    pub fn new(value: u32) -> Self {
        Self { value }
    }

    #[wasm_bindgen(js_name = "value")]
    pub fn value(&self) -> u32 {
        self.value
    }

    /// Takes the JS reference type as a parameter so the `.d.ts` exposes the
    /// `Counter` typescript_type in a function signature.
    #[wasm_bindgen(js_name = "combine")]
    pub fn combine(&self, other: &JsCounter) -> WasmCounter {
        let other: WasmCounter = other.into();
        WasmCounter::new(self.value + other.value)
    }
}

#[js_trait(js_type = JsStorage)]
pub trait Storage {
    #[wasm_bindgen(js_name = "save")]
    async fn js_save(&self, key: String, value: JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = "load")]
    async fn js_load(&self, key: String) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = "keys")]
    fn js_keys(&self) -> Vec<String>;

    #[wasm_bindgen(js_name = "size")]
    fn js_size(&self) -> u32;
}

#[derive(Debug, thiserror::Error, WasmError)]
#[error("storage error: {0}")]
pub struct WasmStorageError(String);

/// A wasm-bindgen-exported function returning a `Result` that can throw the
/// `WasmStorageError` as a named JS Error.
#[wasm_bindgen(js_name = "tryParse")]
pub fn try_parse(input: String) -> Result<u32, JsValue> {
    input
        .parse::<u32>()
        .map_err(|_| WasmStorageError(format!("not a number: {input}")).into())
}
