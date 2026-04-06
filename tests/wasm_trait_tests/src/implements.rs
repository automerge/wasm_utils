//! Test: `#[wasm_implements]` generates valid compile-time witness.

use wasm_bindgen::prelude::*;
use wasm_bindgen_trait::{js_trait, wasm_implements};

// Define the interface
#[js_trait(js_type = JsKvStore)]
pub trait KvStore {
    #[wasm_bindgen(js_name = "put")]
    fn js_put(&self, key: JsValue, value: JsValue);

    #[wasm_bindgen(js_name = "get")]
    fn js_get(&self, key: JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "delete")]
    fn js_delete(&self, key: JsValue) -> bool;
}

// Rust struct that implements the interface
#[wasm_bindgen]
pub struct WasmMemoryKvStore;

#[wasm_implements(KvStore)]
#[wasm_bindgen(js_class = "WasmMemoryKvStore")]
impl WasmMemoryKvStore {
    #[wasm_bindgen(js_name = "put")]
    pub fn js_put(&self, _key: JsValue, _value: JsValue) {}

    #[wasm_bindgen(js_name = "get")]
    pub fn js_get(&self, _key: JsValue) -> JsValue {
        JsValue::UNDEFINED
    }

    #[wasm_bindgen(js_name = "delete")]
    pub fn js_delete(&self, _key: JsValue) -> bool {
        false
    }
}

/// The Rust struct satisfies the trait via the hidden witness.
#[test]
fn rust_export_satisfies_trait() {
    fn assert_kv_store<T: KvStore>() {}
    assert_kv_store::<WasmMemoryKvStore>();
}

/// Both the extern type and the Rust struct implement the trait.
#[test]
fn both_types_implement_trait() {
    fn assert_kv_store<T: KvStore>() {}
    assert_kv_store::<JsKvStore>();
    assert_kv_store::<WasmMemoryKvStore>();
}

/// The trait can be used generically over both types.
#[test]
fn generic_over_both_types() {
    fn use_store<T: KvStore>(s: &T) -> bool {
        s.js_delete(JsValue::NULL)
    }
    let _ = use_store::<JsKvStore>;
    let _ = use_store::<WasmMemoryKvStore>;
}
