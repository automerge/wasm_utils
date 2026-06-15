//! Runtime tests for `#[wasm_refgen]` + `from_js_ref`.
//!
//! These exercise the duck-typed conversion path that cannot run on the host:
//! `try_from_js_value` performs `js_sys::Reflect::has(value, "__wasm_refgen_to{Type}")`
//! and, when present, `unchecked_into` + the injected clone method.

#![allow(clippy::missing_const_for_fn, clippy::must_use_candidate)]

use from_js_ref::{FromJsRef, JsDeref};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_refgen::wasm_refgen;

#[derive(Clone, PartialEq, Debug)]
#[wasm_bindgen(js_name = Counter)]
pub struct WasmCounter {
    value: u32,
}

#[wasm_refgen(js_ref = JsCounter)]
#[wasm_bindgen(js_class = "Counter")]
impl WasmCounter {
    #[wasm_bindgen(constructor)]
    pub fn new(value: u32) -> Self {
        Self { value }
    }

    #[wasm_bindgen(js_name = "value")]
    pub fn value(&self) -> u32 {
        self.value
    }
}

#[derive(Clone, PartialEq, Debug)]
#[wasm_bindgen(js_name = CommitWithBlob)]
pub struct WasmCommitWithBlob {
    data: u32,
}

#[wasm_refgen(js_ref = JsCommitWithBlob)]
#[wasm_bindgen(js_class = "CommitWithBlob")]
impl WasmCommitWithBlob {
    #[wasm_bindgen(constructor)]
    pub fn new(data: u32) -> Self {
        Self { data }
    }
}

/// A JS object that carries the sentinel upcast tag method for `WasmCounter`
/// (`__wasm_refgen_toWasmCounter`), plus the real exported `Counter`, so we can
/// drive the duck-type path from JS-shaped values.
#[wasm_bindgen(inline_js = r#"
export function make_tagged_counter(real) {
    // An ordinary JS object that *quacks* like a Counter: it has the tag
    // method, which is all `Reflect::has` checks for. The method returns the
    // real wasm-bindgen Counter so `from_js_ref` (which calls clone) works.
    return { __wasm_refgen_toWasmCounter: () => real };
}

export function make_untagged_object() {
    return { somethingElse: 42 };
}
"#)]
extern "C" {
    fn make_tagged_counter(real: &JsValue) -> JsValue;
    fn make_untagged_object() -> JsValue;
}

#[wasm_bindgen_test]
fn try_from_js_value_some_for_tagged_object() {
    let real = WasmCounter::new(7);
    let real_js: JsValue = JsValue::from(real);
    let tagged = make_tagged_counter(&real_js);

    let recovered = WasmCounter::try_from_js_value(&tagged);
    assert!(
        recovered.is_some(),
        "try_from_js_value must return Some for an object carrying the upcast tag",
    );
    assert_eq!(recovered.expect("checked Some above").value(), 7);
}

#[wasm_bindgen_test]
fn try_from_js_value_none_for_untagged_object() {
    let untagged = make_untagged_object();
    let recovered = WasmCounter::try_from_js_value(&untagged);
    assert!(
        recovered.is_none(),
        "try_from_js_value must return None when the upcast tag is absent",
    );
}

#[wasm_bindgen_test]
fn try_from_js_value_none_for_primitive() {
    let recovered = WasmCounter::try_from_js_value(&JsValue::from_str("not a counter"));
    assert!(recovered.is_none(), "primitives carry no tag method");
}

#[wasm_bindgen_test]
fn roundtrip_type_to_js_ref_and_back() {
    let original = WasmCounter::new(42);

    // Type -> JsRef (uses From<WasmCounter> for JsCounter -> unchecked_into).
    let js_ref: JsCounter = original.clone().into();

    // JsRef -> Type (uses the injected clone method).
    let restored = WasmCounter::from_js_ref(&js_ref);
    assert_eq!(restored, original);
    assert_eq!(restored.value(), 42);
}

#[wasm_bindgen_test]
fn js_deref_blanket_impl_roundtrips() {
    let original = WasmCounter::new(99);
    let js_ref: JsCounter = original.clone().into();

    let restored: WasmCounter = js_ref.js_deref();
    assert_eq!(restored, original);
}

#[wasm_bindgen_test]
fn from_ref_conversion_roundtrips() {
    let original = WasmCounter::new(5);
    let js_ref: JsCounter = original.clone().into();

    // Exercises `From<&JsCounter> for WasmCounter`.
    let restored: WasmCounter = (&js_ref).into();
    assert_eq!(restored, original);
}

#[wasm_bindgen_test]
fn multi_word_class_roundtrips() {
    let original = WasmCommitWithBlob::new(123);
    let js_ref: JsCommitWithBlob = original.clone().into();
    let restored = WasmCommitWithBlob::from_js_ref(&js_ref);
    assert_eq!(restored, original);
}
