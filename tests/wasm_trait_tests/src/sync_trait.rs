//! Test: `#[js_trait]` with sync methods generates valid code.

use wasm_bindgen::prelude::*;
use wasm_trait::js_trait;

#[js_trait(js_type = JsCounter)]
pub trait Counter {
    #[wasm_bindgen(js_name = "increment")]
    fn js_increment(&self) -> u32;

    #[wasm_bindgen(js_name = "decrement")]
    fn js_decrement(&self) -> u32;

    #[wasm_bindgen(js_name = "value")]
    fn js_value(&self) -> u32;
}

/// The extern type `JsCounter` exists and implements `JsCast`.
#[test]
fn extern_type_is_js_cast() {
    fn assert_js_cast<T: JsCast>() {}
    assert_js_cast::<JsCounter>();
}

/// The extern type implements the generated trait.
#[test]
fn extern_type_implements_trait() {
    fn assert_trait<T: Counter>() {}
    assert_trait::<JsCounter>();
}

/// The trait can be used as a generic bound.
#[test]
fn trait_usable_as_bound() {
    fn use_counter<T: Counter>(c: &T) -> u32 {
        c.js_value()
    }
    // Just verifying the function compiles
    let _ = use_counter::<JsCounter>;
}
