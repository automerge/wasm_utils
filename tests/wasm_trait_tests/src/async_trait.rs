//! Test: `#[js_trait]` with async methods generates valid code.

use wasm_bindgen::prelude::*;
use wasm_bindgen_trait::js_trait;

#[js_trait(js_type = JsStorage)]
pub trait Storage {
    #[wasm_bindgen(js_name = "save")]
    async fn js_save(&self, key: u32, value: JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = "load")]
    async fn js_load(&self, key: u32) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = "delete")]
    async fn js_delete(&self, key: u32) -> Result<(), JsValue>;

    /// Sync method alongside async ones.
    #[wasm_bindgen(js_name = "name")]
    fn js_name(&self) -> String;
}

/// The extern type implements the trait.
#[test]
fn async_extern_type_implements_trait() {
    fn assert_trait<T: Storage>() {}
    assert_trait::<JsStorage>();
}

// Typed error types

/// Async methods can use `js_sys::Error` as the error type instead of `JsValue`.
#[js_trait(js_type = JsTypedApi)]
pub trait TypedApi {
    #[wasm_bindgen(js_name = "fetchData")]
    async fn js_fetch_data(&self) -> Result<js_sys::Uint8Array, js_sys::Error>;

    #[wasm_bindgen(js_name = "send")]
    async fn js_send(&self, data: js_sys::Uint8Array) -> Result<(), js_sys::Error>;

    #[wasm_bindgen(js_name = "fetchAny")]
    async fn js_fetch_any(&self) -> Result<JsValue, JsValue>;
}

/// Typed error types compile correctly.
#[test]
fn typed_error_compiles() {
    fn assert_trait<T: TypedApi>() {}
    assert_trait::<JsTypedApi>();
}

// An async method borrowing a parameter: the generated `async move` body
// captures `key` under the RPITIT's lifetime.
#[js_trait(js_type = JsLookup)]
pub trait Lookup {
    #[wasm_bindgen(js_name = "find")]
    async fn js_find(&self, key: &str) -> Result<JsValue, JsValue>;
}

#[test]
fn async_method_with_borrowed_param_compiles() {
    fn assert_trait<T: Lookup>() {}
    assert_trait::<JsLookup>();
}
