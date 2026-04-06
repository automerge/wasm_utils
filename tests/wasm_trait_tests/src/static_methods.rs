//! Test: static methods (no `&self` receiver) work in both macros.

use wasm_bindgen::prelude::*;
use wasm_trait::{js_trait, wasm_implements};

#[js_trait(js_type = JsFactory)]
pub trait Factory {
    #[wasm_bindgen(js_name = "create")]
    fn js_create(name: String) -> JsValue;

    /// Instance method alongside static.
    #[wasm_bindgen(js_name = "describe")]
    fn js_describe(&self) -> String;
}

#[wasm_bindgen]
pub struct WasmFactory;

#[wasm_implements(Factory)]
#[wasm_bindgen(js_class = "WasmFactory")]
impl WasmFactory {
    #[wasm_bindgen(js_name = "create")]
    pub fn js_create(_name: String) -> JsValue {
        JsValue::NULL
    }

    #[wasm_bindgen(js_name = "describe")]
    pub fn js_describe(&self) -> String {
        String::from("factory")
    }
}

/// Both types implement the trait (including static methods).
#[test]
fn static_methods_compile() {
    fn assert_factory<T: Factory>() {}
    assert_factory::<JsFactory>();
    assert_factory::<WasmFactory>();
}
