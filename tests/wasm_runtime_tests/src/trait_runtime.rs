//! Runtime tests for `#[js_trait]` and `#[wasm_implements]`.
//!
//! `#[js_trait]` generates `impl Trait for ExternType` whose method bodies call
//! across the JS boundary (sync delegation, or async via `JsFuture::from` on a
//! returned promise). Whether that delegation actually marshals values and maps
//! promise resolve/reject onto `Result` is only observable at runtime.

#![allow(
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::unused_self,
    dead_code
)]

use js_sys::Reflect;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_trait::{js_trait, wasm_implements};

#[js_trait(js_type = JsCalculator)]
pub trait Calculator {
    #[wasm_bindgen(js_name = "add")]
    fn js_add(&self, a: JsValue, b: JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "label")]
    fn js_label(&self) -> JsValue;
}

#[js_trait(js_type = JsAsyncStore)]
pub trait AsyncStore {
    #[wasm_bindgen(js_name = "load")]
    async fn js_load(&self, key: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = "failing")]
    async fn js_failing(&self) -> Result<JsValue, JsValue>;
}

#[wasm_bindgen(inline_js = r#"
export function make_calculator() {
    return {
        add: (a, b) => a + b,
        label: () => "js-calc",
    };
}

export function make_async_store() {
    return {
        load: async (key) => "value-for-" + key,
        failing: async () => { throw new Error("boom"); },
    };
}
"#)]
extern "C" {
    fn make_calculator() -> JsCalculator;
    fn make_async_store() -> JsAsyncStore;
}

#[wasm_bindgen_test]
fn sync_delegation_marshals_values() {
    let calc = make_calculator();
    let sum = calc.js_add(JsValue::from_f64(2.0), JsValue::from_f64(40.0));
    assert_eq!(sum.as_f64(), Some(42.0));

    let label = calc.js_label();
    assert_eq!(label.as_string().as_deref(), Some("js-calc"));
}

#[wasm_bindgen_test]
async fn async_resolution_maps_to_ok() {
    let store = make_async_store();
    let loaded = store
        .js_load(JsValue::from_str("k"))
        .await
        .expect("resolved promise must map to Ok");
    assert_eq!(loaded.as_string().as_deref(), Some("value-for-k"));
}

#[wasm_bindgen_test]
async fn async_rejection_maps_to_err() {
    let store = make_async_store();
    let err = store
        .js_failing()
        .await
        .expect_err("a rejected promise must map to Err(JsValue)");
    let err: js_sys::Error = err
        .dyn_into()
        .expect("the rejection value is passed through");
    assert_eq!(err.message(), "boom");
}

#[wasm_bindgen]
pub struct WasmRustCalculator;

#[wasm_bindgen(js_class = "WasmRustCalculator")]
impl WasmRustCalculator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self
    }
}

#[wasm_implements(Calculator)]
#[wasm_bindgen(js_class = "WasmRustCalculator")]
impl WasmRustCalculator {
    #[wasm_bindgen(js_name = "add")]
    pub fn js_add(&self, a: JsValue, b: JsValue) -> JsValue {
        let sum = a.as_f64().unwrap_or(0.0) + b.as_f64().unwrap_or(0.0);
        JsValue::from_f64(sum)
    }

    #[wasm_bindgen(js_name = "label")]
    pub fn js_label(&self) -> JsValue {
        JsValue::from_str("rust-calc")
    }
}

#[wasm_bindgen(inline_js = r#"
export function drive_calculator(calc) {
    // Call the Rust-exported object purely through its JS surface.
    const sum = calc.add(10, 5);
    const label = calc.label();
    return sum + ":" + label;
}
"#)]
extern "C" {
    fn drive_calculator(calc: &JsValue) -> JsValue;
}

#[wasm_bindgen_test]
fn rust_export_callable_from_js() {
    let calc = WasmRustCalculator::new();
    let js: JsValue = calc.into();
    let result = drive_calculator(&js);
    assert_eq!(result.as_string().as_deref(), Some("15:rust-calc"));
}

#[wasm_bindgen_test]
fn rust_export_implements_trait_at_runtime() {
    // Drives the `#[wasm_implements]` witness: `WasmRustCalculator` satisfies
    // `Calculator` through its own inherent methods, not the JS extern type.
    fn use_calc<C: Calculator>(c: &C) -> Option<f64> {
        c.js_add(JsValue::from_f64(1.0), JsValue::from_f64(2.0))
            .as_f64()
    }

    assert_eq!(use_calc(&WasmRustCalculator::new()), Some(3.0));
}

#[wasm_bindgen_test]
fn rust_export_carries_runtime_tag() {
    let js: JsValue = WasmRustCalculator::new().into();
    let tag: js_sys::Function = Reflect::get(&js, &JsValue::from_str("__wasm_impl_Calculator"))
        .expect("Reflect::get on an exported object must not throw")
        .dyn_into()
        .expect("`#[wasm_implements(Calculator)]` must inject a callable tag");
    assert_eq!(
        tag.call0(&js).expect("the tag must not throw"),
        JsValue::TRUE,
        "the tag reports conformance by returning true",
    );
}
