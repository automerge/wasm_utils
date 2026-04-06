//! Test: `js_name` parameter overrides the TS interface name.

use wasm_trait::js_trait;

#[js_trait(js_type = JsLogger, js_name = ConsoleLogger)]
pub trait Logger {
    #[wasm_bindgen(js_name = "log")]
    fn js_log(&self, message: String);

    #[wasm_bindgen(js_name = "level")]
    fn js_level(&self) -> u32;
}

/// The extern type exists and implements the trait.
#[test]
fn js_name_override_compiles() {
    fn assert_trait<T: Logger + ::wasm_bindgen::JsCast>() {}
    assert_trait::<JsLogger>();
}
