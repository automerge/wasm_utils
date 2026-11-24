//! Test: `#[js_trait]` with sync methods generates valid code.

use wasm_bindgen_trait::js_trait;

#[js_trait(js_type = JsCounter)]
pub trait Counter {
    #[wasm_bindgen(js_name = "increment")]
    fn js_increment(&self) -> u32;

    #[wasm_bindgen(js_name = "decrement")]
    fn js_decrement(&self) -> u32;

    #[wasm_bindgen(js_name = "value")]
    fn js_value(&self) -> u32;
}

/// The extern type implements the generated trait.
#[test]
fn extern_type_implements_trait() {
    fn assert_trait<T: Counter>() {}
    assert_trait::<JsCounter>();
}
