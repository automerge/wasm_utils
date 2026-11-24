//! Test: `js_name` parameter overrides the TS interface name.

use wasm_bindgen_trait::js_trait;

#[js_trait(js_type = JsLogger, js_name = ConsoleLogger)]
pub trait Logger {
    #[wasm_bindgen(js_name = "log")]
    fn js_log(&self, message: String);

    #[wasm_bindgen(js_name = "level")]
    fn js_level(&self) -> u32;
}
