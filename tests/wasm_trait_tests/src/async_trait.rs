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

/// Async trait methods return `impl Future`.
#[test]
fn async_methods_return_future() {
    fn assert_future<T: core::future::Future>(_t: &T) {}
    fn check_save<T: Storage>(t: &T) {
        let fut = t.js_save(0, JsValue::NULL);
        assert_future(&fut);
    }
    let _ = check_save::<JsStorage>;
}

/// Mixed sync and async methods work together.
#[test]
fn mixed_sync_async() {
    fn use_storage<T: Storage>(s: &T) -> String {
        s.js_name()
    }
    let _ = use_storage::<JsStorage>;
}

// ── Typed error types ────────────────────────────────────────────

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

/// Typed errors work in generic bounds.
#[test]
fn typed_error_in_generic_context() {
    fn use_api<T: TypedApi>(t: &T) {
        let _fut = t.js_fetch_data();
        let _fut2 = t.js_send(js_sys::Uint8Array::new_with_length(0));
    }
    let _ = use_api::<JsTypedApi>;
}
