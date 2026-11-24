//! Test: `#[wasm_implements]` generates valid compile-time witness.

use core::cell::RefCell;

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
#[derive(Debug, Default)]
#[wasm_bindgen]
pub struct WasmMemoryKvStore {
    entries: RefCell<Vec<(JsValue, JsValue)>>,
}

#[wasm_implements(KvStore)]
#[wasm_bindgen(js_class = "WasmMemoryKvStore")]
impl WasmMemoryKvStore {
    #[wasm_bindgen(js_name = "put")]
    pub fn js_put(&self, key: JsValue, value: JsValue) {
        self.entries.borrow_mut().push((key, value));
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "`KvStore` takes the key by value"
    )]
    #[wasm_bindgen(js_name = "get")]
    pub fn js_get(&self, key: JsValue) -> JsValue {
        self.entries
            .borrow()
            .iter()
            .find(|(k, _)| *k == key)
            .map_or(JsValue::UNDEFINED, |(_, v)| v.clone())
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "`KvStore` takes the key by value"
    )]
    #[wasm_bindgen(js_name = "delete")]
    pub fn js_delete(&self, key: JsValue) -> bool {
        let mut entries = self.entries.borrow_mut();
        let before = entries.len();
        entries.retain(|(k, _)| *k != key);
        entries.len() != before
    }
}

/// Both the extern type and the Rust struct implement the trait.
#[test]
fn both_types_implement_trait() {
    fn assert_kv_store<T: KvStore>() {}
    assert_kv_store::<JsKvStore>();
    assert_kv_store::<WasmMemoryKvStore>();
}

// Bare-identifier `js_name` (no quotes) is valid wasm-bindgen syntax on both
// the trait and the impl side, and must satisfy the conformance check.
#[js_trait(js_type = JsBareCounter)]
pub trait BareCounter {
    #[wasm_bindgen(js_name = getCount)]
    fn get_count(&self) -> u32;
}

#[expect(
    missing_copy_implementations,
    reason = "exported to JS; its `&self` methods come from the trait"
)]
#[derive(Debug, Default)]
#[wasm_bindgen]
pub struct WasmBareCounter {
    count: u32,
}

#[wasm_implements(BareCounter)]
#[wasm_bindgen(js_class = "WasmBareCounter")]
impl WasmBareCounter {
    #[expect(
        clippy::missing_const_for_fn,
        reason = "wasm-bindgen can't export a `const fn`"
    )]
    #[wasm_bindgen(js_name = getCount)]
    pub fn get_count(&self) -> u32 {
        self.count
    }
}

#[test]
fn bare_identifier_js_name_conforms() {
    fn assert_trait<T: BareCounter>() {}
    assert_trait::<WasmBareCounter>();
}

// The witness for an async method with a borrowed parameter must compile too.
#[js_trait(js_type = JsFinder)]
pub trait Finder {
    #[wasm_bindgen(js_name = "find")]
    async fn js_find(&self, key: &str) -> Result<JsValue, JsValue>;
}

#[expect(
    missing_copy_implementations,
    reason = "exported to JS; its `&self` methods come from the trait"
)]
#[derive(Debug)]
#[wasm_bindgen]
pub struct WasmFinder;

#[wasm_implements(Finder)]
#[wasm_bindgen(js_class = "WasmFinder")]
impl WasmFinder {
    #[expect(
        clippy::unused_async,
        reason = "must be `async` to match the trait's `async fn`"
    )]
    #[wasm_bindgen(js_name = "find")]
    pub async fn js_find(&self, key: &str) -> Result<JsValue, JsValue> {
        Ok(JsValue::from_str(key))
    }
}

#[test]
fn async_borrowed_param_conforms() {
    fn assert_trait<T: Finder>() {}
    assert_trait::<WasmFinder>();
}

// Parameter patterns wasm-bindgen accepts (`mut v`, `_`) must not break the
// witness, which forwards every parameter to the inherent method.
#[js_trait(js_type = JsSetter)]
pub trait Setter {
    #[wasm_bindgen(js_name = "set")]
    fn js_set(&self, value: u32, flag: bool) -> u32;
}

#[expect(
    missing_copy_implementations,
    reason = "exported to JS; its `&self` methods come from the trait"
)]
#[derive(Debug, Default)]
#[wasm_bindgen]
pub struct WasmSetter {
    step: u32,
}

#[wasm_implements(Setter)]
#[wasm_bindgen(js_class = "WasmSetter")]
impl WasmSetter {
    #[expect(
        clippy::missing_const_for_fn,
        reason = "wasm-bindgen can't export a `const fn`"
    )]
    #[wasm_bindgen(js_name = "set")]
    pub fn js_set(&self, mut value: u32, _: bool) -> u32 {
        value += self.step;
        value
    }
}

#[test]
fn witness_forwards_any_parameter_pattern() {
    fn assert_trait<T: Setter>() {}
    assert_trait::<WasmSetter>();
}
