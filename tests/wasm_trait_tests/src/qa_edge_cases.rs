//! QA edge-case tests for `wasm_trait` proc macros.
//!
//! Each test targets a specific edge case. Because these are compile-time
//! assertion tests (`wasm_bindgen` extern types can't actually run on native),
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
use wasm_bindgen_trait::{js_trait, wasm_implements};

/// The required JS names, in declaration order, from a `__JS_METHODS` table.
#[cfg(test)]
fn js_names(table: &[(&'static str, &'static str, &'static str)]) -> Vec<&'static str> {
    table.iter().map(|&(_, js_name, _)| js_name).collect()
}

// 1. Empty trait — zero methods

#[js_trait(js_type = JsEmpty)]
pub trait Empty {}

#[test]
fn empty_trait_compiles() {
    fn assert_trait<T: Empty>() {}
    assert_trait::<JsEmpty>();
}

// 2. Single method trait — minimal case

#[js_trait(js_type = JsPing)]
pub trait Ping {
    #[wasm_bindgen(js_name = "ping")]
    fn js_ping(&self) -> bool;
}

// 3. Method with many parameters (5+)

#[js_trait(js_type = JsManyParams)]
pub trait ManyParams {
    #[wasm_bindgen(js_name = "doStuff")]
    fn js_do_stuff(&self, a: u32, b: u32, c: String, d: bool, e: JsValue, f: u32) -> JsValue;
}

// 4. Method returning Option<JsValue>

#[js_trait(js_type = JsMaybeValue)]
pub trait MaybeValue {
    #[wasm_bindgen(js_name = "find")]
    fn js_find(&self, key: u32) -> Option<JsValue>;
}

// 5. Method with reference params (&str, &JsValue)
//
// NOTE: &str and &JsValue are valid in wasm_bindgen extern "C" blocks.
// &[u8] is NOT valid in extern blocks (wasm_bindgen rejects it), so we
// only test the reference types wasm_bindgen actually supports.

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

// 6. Multiple #[wasm_implements] on same struct (separate impl blocks)

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

#[expect(
    missing_copy_implementations,
    reason = "exported to JS; its `&self` methods come from the trait"
)]
#[derive(Debug)]
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
    assert_alpha::<JsAlpha>();
    assert_beta::<JsBeta>();
}

// 7. Trait with doc comments — verify they survive macro expansion

/// This doc comment should survive on the trait.
#[js_trait(js_type = JsDocumented)]
pub trait Documented {
    /// Method-level doc comment — should also survive.
    #[wasm_bindgen(js_name = "info")]
    fn js_info(&self) -> String;
}

// 8. Method with explicit js_name attr

#[js_trait(js_type = JsNoWbAttr)]
pub trait NoWbAttr {
    #[wasm_bindgen(js_name = "bareMethod")]
    fn js_bare_method(&self) -> u32;
}

// 9. Trait method names that collide with __wasm_trait_ prefix

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

// 10. Very long method names

#[js_trait(js_type = JsLongName)]
pub trait LongName {
    #[wasm_bindgen(
        js_name = "thisIsAnExtremelyLongMethodNameThatTestsWhetherTheMacroHandlesVeryLongIdentifiersCorrectly"
    )]
    fn js_this_is_an_extremely_long_method_name_that_tests_whether_the_macro_handles_very_long_identifiers_correctly(
        &self,
    ) -> u32;
}

// 11. Async method returning Result<JsValue, JsValue>

#[js_trait(js_type = JsIdentityAsync)]
pub trait IdentityAsync {
    #[wasm_bindgen(js_name = "fetch")]
    async fn js_fetch(&self, url: String) -> Result<JsValue, JsValue>;
}

// 12. Catch method (sync) returning Result<JsValue, JsValue>

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

// 13. JS name check — happy path

#[js_trait(js_type = JsTransportQa)]
pub trait TransportQa {
    #[wasm_bindgen(js_name = "sendBytes")]
    fn js_send_bytes(&self, data: JsValue) -> bool;

    #[wasm_bindgen(js_name = "recvBytes")]
    fn js_recv_bytes(&self) -> JsValue;
}

#[expect(
    missing_copy_implementations,
    reason = "exported to JS; its `&self` methods come from the trait"
)]
#[derive(Debug)]
#[wasm_bindgen]
pub struct WasmTransportQa;

#[wasm_implements(TransportQa)]
#[wasm_bindgen(js_class = "WasmTransportQa")]
impl WasmTransportQa {
    #[wasm_bindgen(js_name = "sendBytes")]
    pub fn js_send_bytes(&self, _data: JsValue) -> bool {
        true
    }

    #[wasm_bindgen(js_name = "recvBytes")]
    pub fn js_recv_bytes(&self) -> JsValue {
        JsValue::NULL
    }
}

#[test]
fn js_interface_const_has_expected_names() {
    assert_eq!(
        js_names(<JsTransportQa as TransportQa>::__JS_METHODS),
        ["sendBytes", "recvBytes"],
    );
}

// 15. JS name check — js_name required on trait methods

#[js_trait(js_type = JsBareMethodQa)]
pub trait BareMethodQa {
    #[wasm_bindgen(js_name = "bareFn")]
    fn js_bare_fn(&self) -> u32;
}

// 16. async fn in trait — mock uses plain async fn

#[js_trait(js_type = JsAsyncQa)]
pub trait AsyncQa {
    #[wasm_bindgen(js_name = "fetchData")]
    async fn js_fetch_data(&self, url: String) -> Result<JsValue, JsValue>;
}

#[allow(dead_code)] // Used only in type-level assertions
struct MockAsyncQa;

impl AsyncQa for MockAsyncQa {
    async fn js_fetch_data(&self, _url: String) -> Result<JsValue, JsValue> {
        Ok(JsValue::NULL)
    }
}

// 17. Typed error types — js_sys::Error

#[js_trait(js_type = JsTypedErrQa)]
pub trait TypedErrQa {
    #[wasm_bindgen(js_name = "saveData")]
    async fn js_save_data(&self, data: JsValue) -> Result<(), js_sys::Error>;

    #[wasm_bindgen(js_name = "loadData")]
    async fn js_load_data(&self, key: String) -> Result<js_sys::Uint8Array, js_sys::Error>;
}

#[allow(dead_code)] // Used only in type-level assertions
struct MockTypedErrQa;

impl TypedErrQa for MockTypedErrQa {
    async fn js_save_data(&self, _data: JsValue) -> Result<(), js_sys::Error> {
        Ok(())
    }

    async fn js_load_data(&self, _key: String) -> Result<js_sys::Uint8Array, js_sys::Error> {
        Ok(js_sys::Uint8Array::new_with_length(0))
    }
}

// 18. Mixed trait (async + sync + static) with wasm_implements

#[js_trait(js_type = JsMixedQa)]
pub trait MixedQa {
    #[wasm_bindgen(js_name = "name")]
    fn js_name(&self) -> String;

    #[wasm_bindgen(js_name = "fetchItem")]
    async fn js_fetch_item(&self, id: u32) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = "create")]
    fn js_create(label: String) -> JsValue;
}

#[expect(
    missing_copy_implementations,
    reason = "exported to JS; its `&self` methods come from the trait"
)]
#[derive(Debug)]
#[wasm_bindgen]
pub struct WasmMixedQa;

#[wasm_implements(MixedQa)]
#[wasm_bindgen(js_class = "WasmMixedQa")]
impl WasmMixedQa {
    #[wasm_bindgen(js_name = "name")]
    pub fn js_name(&self) -> String {
        "mixed".into()
    }

    #[expect(
        clippy::unused_async,
        reason = "must be `async` to match the trait's `async fn`"
    )]
    #[wasm_bindgen(js_name = "fetchItem")]
    pub async fn js_fetch_item(&self, _id: u32) -> Result<JsValue, JsValue> {
        Ok(JsValue::NULL)
    }

    #[wasm_bindgen(js_name = "create")]
    pub fn js_create(_label: String) -> JsValue {
        JsValue::NULL
    }
}

#[test]
fn mixed_trait_end_to_end() {
    fn assert_trait<T: MixedQa>() {}
    assert_trait::<JsMixedQa>();
    assert_trait::<WasmMixedQa>();
}

#[test]
fn mixed_js_interface_const_correct() {
    assert_eq!(
        js_names(<JsMixedQa as MixedQa>::__JS_METHODS),
        ["name", "fetchItem", "create"],
    );
}

// 19. Module-qualified trait path in wasm_implements

pub mod inner_module {
    use wasm_bindgen_trait::js_trait;

    #[js_trait(js_type = JsInnerTrait)]
    pub trait InnerTrait {
        #[wasm_bindgen(js_name = "doWork")]
        fn js_do_work(&self) -> bool;
    }
}

#[expect(
    missing_copy_implementations,
    reason = "exported to JS; its `&self` methods come from the trait"
)]
#[derive(Debug)]
#[wasm_bindgen]
pub struct WasmInnerImpl;

// Module-qualified path — the macro accesses the associated const via
// `<WasmInnerImpl as inner_module::InnerTrait>::__JS_METHODS`.
#[wasm_implements(inner_module::InnerTrait)]
#[wasm_bindgen(js_class = "WasmInnerImpl")]
impl WasmInnerImpl {
    #[wasm_bindgen(js_name = "doWork")]
    pub fn js_do_work(&self) -> bool {
        true
    }
}

#[test]
fn module_qualified_path_compiles() {
    fn assert_trait<T: inner_module::InnerTrait>() {}
    assert_trait::<inner_module::JsInnerTrait>();
    assert_trait::<WasmInnerImpl>();
}

// 20. TS type mapping stress test

#[js_trait(js_type = JsTsStress)]
pub trait TsStress {
    #[wasm_bindgen(js_name = "maybeName")]
    fn js_maybe_name(&self) -> Option<String>;

    #[wasm_bindgen(js_name = "sendVoid")]
    async fn js_send_void(&self) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = "fetchArray")]
    async fn js_fetch_array(&self) -> Result<js_sys::Array, JsValue>;

    #[wasm_bindgen(js_name = "isReady")]
    fn js_is_ready(&self) -> bool;

    #[wasm_bindgen(js_name = "reset")]
    fn js_reset(&self);
}

#[allow(dead_code)] // Used only in type-level assertions
struct MockTsStress;

impl TsStress for MockTsStress {
    fn js_maybe_name(&self) -> Option<String> {
        Some("test".into())
    }

    async fn js_send_void(&self) -> Result<(), JsValue> {
        Ok(())
    }

    async fn js_fetch_array(&self) -> Result<js_sys::Array, JsValue> {
        Ok(js_sys::Array::new())
    }

    fn js_is_ready(&self) -> bool {
        true
    }

    fn js_reset(&self) {}
}

// Trait with supertraits compiles

#[js_trait(js_type = JsWithSuper)]
pub trait WithSuper: Send + Sync {
    #[wasm_bindgen(js_name = "id")]
    fn js_id(&self) -> u32;
}

#[test]
fn supertrait_compiles() {
    fn assert_trait<T: WithSuper>() {}
    assert_trait::<JsWithSuper>();
}
