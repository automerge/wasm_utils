//! QA edge-case tests for `wasm_trait` proc macros.
//!
//! Each test targets a specific edge case. Because these are compile-time
//! assertion tests (wasm_bindgen extern types can't actually run on native),
//! "passing" means the macro expansion compiles without error.

#![allow(
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value,
    clippy::unused_self,
    missing_docs,
    unreachable_pub
)]

use wasm_bindgen::prelude::*;
use wasm_trait::{js_trait, wasm_implements};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 1. Empty trait — zero methods
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[js_trait(js_type = JsEmpty)]
pub trait Empty {}

#[test]
fn empty_trait_compiles() {
    fn assert_js_cast<T: JsCast>() {}
    assert_js_cast::<JsEmpty>();

    fn assert_trait<T: Empty>() {}
    assert_trait::<JsEmpty>();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 2. Single method trait — minimal case
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[js_trait(js_type = JsPing)]
pub trait Ping {
    #[wasm_bindgen(js_name = "ping")]
    fn js_ping(&self) -> bool;
}

#[test]
fn single_method_trait_compiles() {
    fn assert_trait<T: Ping>() {}
    assert_trait::<JsPing>();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 3. Method with many parameters (5+)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[js_trait(js_type = JsManyParams)]
pub trait ManyParams {
    #[wasm_bindgen(js_name = "doStuff")]
    fn js_do_stuff(&self, a: u32, b: u32, c: String, d: bool, e: JsValue, f: u32) -> JsValue;
}

#[test]
fn many_params_compiles() {
    fn assert_trait<T: ManyParams>() {}
    assert_trait::<JsManyParams>();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 4. Method returning Option<JsValue>
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[js_trait(js_type = JsMaybeValue)]
pub trait MaybeValue {
    #[wasm_bindgen(js_name = "find")]
    fn js_find(&self, key: u32) -> Option<JsValue>;
}

#[test]
fn option_return_compiles() {
    fn assert_trait<T: MaybeValue>() {}
    assert_trait::<JsMaybeValue>();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 5. Method with reference params (&str, &[u8], &JsValue)
//
// NOTE: &str and &JsValue are valid in wasm_bindgen extern "C" blocks.
// &[u8] is NOT valid in extern blocks (wasm_bindgen rejects it), so we
// only test the reference types wasm_bindgen actually supports.
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[js_trait(js_type = JsRefParams)]
pub trait RefParams {
    #[wasm_bindgen(js_name = "acceptStr")]
    fn js_accept_str(&self, s: &str) -> String;

    #[wasm_bindgen(js_name = "acceptJsValue")]
    fn js_accept_js_value(&self, v: &JsValue) -> bool;
}

#[test]
fn ref_params_compiles() {
    fn assert_trait<T: RefParams>() {}
    assert_trait::<JsRefParams>();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 6. Multiple #[wasm_implements] on same struct
//
// Tests that a single struct can implement multiple js_trait-defined traits.
// Each #[wasm_implements] goes on a _separate_ impl block (since Rust doesn't
// support multiple proc-macro attrs that each want to modify the same block).
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[js_trait(js_type = JsAlpha)]
pub trait Alpha {
    #[wasm_bindgen(js_name = "alpha")]
    fn js_alpha(&self) -> u32;
}

#[js_trait(js_type = JsBeta)]
pub trait Beta {
    #[wasm_bindgen(js_name = "beta")]
    fn js_beta(&self) -> u32;
}

#[wasm_bindgen]
pub struct WasmDualImpl;

#[wasm_implements(Alpha)]
#[wasm_bindgen(js_class = "WasmDualImpl")]
impl WasmDualImpl {
    #[wasm_bindgen(js_name = "alpha")]
    pub fn js_alpha(&self) -> u32 {
        1
    }
}

#[wasm_implements(Beta)]
#[wasm_bindgen(js_class = "WasmDualImpl")]
impl WasmDualImpl {
    #[wasm_bindgen(js_name = "beta")]
    pub fn js_beta(&self) -> u32 {
        2
    }
}

#[test]
fn multiple_wasm_implements_compiles() {
    fn assert_alpha<T: Alpha>() {}
    fn assert_beta<T: Beta>() {}

    assert_alpha::<WasmDualImpl>();
    assert_beta::<WasmDualImpl>();

    // Extern types also implement their respective traits
    assert_alpha::<JsAlpha>();
    assert_beta::<JsBeta>();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 7. Trait with doc comments — verify they survive macro expansion
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// This doc comment should survive on the trait.
#[js_trait(js_type = JsDocumented)]
pub trait Documented {
    /// Method-level doc comment — should also survive.
    #[wasm_bindgen(js_name = "info")]
    fn js_info(&self) -> String;
}

#[test]
fn doc_comments_survive() {
    // If this compiles, the doc attrs were preserved (or at least not
    // rejected). We can't easily inspect doc attrs at runtime, but we
    // can at least verify the trait is usable.
    fn assert_trait<T: Documented>() {}
    assert_trait::<JsDocumented>();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 8. Method with no #[wasm_bindgen] attr
//
// The macro should still work — the extern fn just won't have a js_name
// override. The extern fn name will be the __wasm_trait_-prefixed Rust name.
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[js_trait(js_type = JsNoWbAttr)]
pub trait NoWbAttr {
    fn js_bare_method(&self) -> u32;
}

#[test]
fn no_wasm_bindgen_attr_compiles() {
    fn assert_trait<T: NoWbAttr>() {}
    assert_trait::<JsNoWbAttr>();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 9. Trait method names that collide with __wasm_trait_ prefix
//
// The extern fn naming scheme prepends __wasm_trait_ to the method name.
// If the method is already named __wasm_trait_something, the extern fn
// becomes __wasm_trait___wasm_trait_something. This is ugly but valid — the
// double prefix is just an identifier, not a collision in the technical sense.
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[js_trait(js_type = JsPrefixCollide)]
pub trait PrefixCollide {
    #[wasm_bindgen(js_name = "something")]
    fn __wasm_trait_something(&self) -> u32;
}

#[test]
fn prefix_collision_compiles() {
    fn assert_trait<T: PrefixCollide>() {}
    assert_trait::<JsPrefixCollide>();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 10. Very long method names
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[js_trait(js_type = JsLongName)]
pub trait LongName {
    #[wasm_bindgen(
        js_name = "thisIsAnExtremelyLongMethodNameThatTestsWhetherTheMacroHandlesVeryLongIdentifiersCorrectly"
    )]
    fn js_this_is_an_extremely_long_method_name_that_tests_whether_the_macro_handles_very_long_identifiers_correctly(
        &self,
    ) -> u32;
}

#[test]
fn very_long_method_name_compiles() {
    fn assert_trait<T: LongName>() {}
    assert_trait::<JsLongName>();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 11. Async method returning Result<JsValue, JsValue>
//
// Both Ok and Err arms are JsValue. The macro uses `unchecked_into` for
// JsValue → JsValue, which is valid because JsValue implements JsCast and
// unchecked_into on itself is identity (modulo a no-op cast).
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[js_trait(js_type = JsIdentityAsync)]
pub trait IdentityAsync {
    #[wasm_bindgen(js_name = "fetch")]
    async fn js_fetch(&self, url: String) -> Result<JsValue, JsValue>;
}

#[test]
fn async_result_jsvalue_jsvalue_compiles() {
    fn assert_trait<T: IdentityAsync>() {}
    assert_trait::<JsIdentityAsync>();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 12. Catch method (sync) returning Result<JsValue, JsValue>
//
// The `catch` attribute makes wasm_bindgen's extern fn return Result.
// The macro forwards `catch` to the extern block. The trait method signature
// keeps the Result as-is. The impl block delegates directly (sync, no
// JsFuture wrapping).
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[js_trait(js_type = JsCatcher)]
pub trait Catcher {
    #[wasm_bindgen(catch, js_name = "tryParse")]
    fn js_try_parse(&self, input: String) -> Result<JsValue, JsValue>;
}

#[test]
fn sync_catch_result_compiles() {
    fn assert_trait<T: Catcher>() {}
    assert_trait::<JsCatcher>();
}
