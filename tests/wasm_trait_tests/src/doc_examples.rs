//! Compilable versions of the `wasm_bindgen_trait` crate-level doc examples.
//!
//! These live here instead of as doc-tests because `wasm_bindgen_trait` is a
//! proc-macro crate. Proc-macro crates can only export macros — their
//! `Cargo.toml` dependencies (`proc-macro2`, `quote`, `syn`) are for the
//! macro implementation, not available to doc-test compilation. Any doc
//! example referencing `wasm_bindgen::JsValue` or `js_sys::Promise` would
//! fail with "unresolved module." This integration test crate has those
//! dependencies, so the same code compiles here.
//!
//! The source doc comments keep `ignore` markers so the examples still
//! render in `rustdoc`.

#[allow(unused_imports)]
use wasm_bindgen::prelude::*;
use wasm_bindgen_trait::{js_trait, wasm_implements};

// ── Doc example: "Quick Start §1 — Define a JS interface" (line 15) ─

#[js_trait(js_type = JsStorage)]
pub trait Storage {
    #[wasm_bindgen(js_name = "save")]
    async fn js_save(&self, key: String, value: JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = "load")]
    async fn js_load(&self, key: String) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = "name")]
    fn js_name(&self) -> String;
}

#[test]
fn js_trait_generates_extern_type_and_trait() {
    fn assert_storage<T: Storage>() {}
    fn assert_js_cast<T: wasm_bindgen::JsCast>() {}

    assert_storage::<JsStorage>();
    assert_js_cast::<JsStorage>();
}

// ── Doc example: "Quick Start §2 — Use the JS object" (line 39) ────

#[allow(dead_code)]
fn accept_storage(s: &JsStorage) {
    let _name = s.js_name();
}

#[allow(dead_code)]
async fn use_storage(s: &impl Storage) {
    s.js_save("key".into(), JsValue::from_str("value"))
        .await
        .expect("save failed");
}

#[test]
fn trait_usable_as_bound() {
    fn assert_bound<T: Storage>() {}
    assert_bound::<JsStorage>();
    // accept_storage and use_storage compile — that's the test
}

// ── Doc example: "Quick Start §3 — wasm_implements" (line 55) ──────

#[wasm_bindgen]
pub struct WasmMemoryStorage;

#[wasm_implements(Storage)]
#[wasm_bindgen(js_class = "WasmMemoryStorage")]
impl WasmMemoryStorage {
    #[wasm_bindgen(js_name = "save")]
    pub async fn js_save(&self, _key: String, _value: JsValue) -> Result<(), JsValue> {
        Ok(())
    }

    #[wasm_bindgen(js_name = "load")]
    pub async fn js_load(&self, _key: String) -> Result<JsValue, JsValue> {
        Ok(JsValue::UNDEFINED)
    }

    #[wasm_bindgen(js_name = "name")]
    pub fn js_name(&self) -> String {
        "memory".into()
    }
}

#[test]
fn wasm_implements_conformance_compiles() {
    fn assert_storage<T: Storage>() {}
    assert_storage::<JsStorage>();
    // WasmMemoryStorage compiles with #[wasm_implements] — that's the test
}

// ── Doc example: "Bridging to Domain Traits" (line 91) ──────────────

#[allow(dead_code)]
trait DocumentStore {
    async fn save(&self, id: &str, content: &[u8]) -> Result<(), DomainError>;

    async fn load(&self, id: &str) -> Result<Vec<u8>, DomainError>;
}

#[allow(dead_code)]
struct DomainError(String);

impl DomainError {
    #[allow(dead_code)]
    fn from_js(val: JsValue) -> Self {
        Self(val.as_string().unwrap_or_else(|| "unknown error".into()))
    }
}

impl DocumentStore for JsStorage {
    async fn save(&self, id: &str, content: &[u8]) -> Result<(), DomainError> {
        let js_value = js_sys::Uint8Array::from(content).into();
        self.js_save(id.into(), js_value)
            .await
            .map_err(DomainError::from_js)
    }

    async fn load(&self, id: &str) -> Result<Vec<u8>, DomainError> {
        let result = self
            .js_load(id.into())
            .await
            .map_err(DomainError::from_js)?;

        let array = js_sys::Uint8Array::new(&result);
        Ok(array.to_vec())
    }
}

#[test]
fn domain_bridge_compiles() {
    fn assert_doc_store<T: DocumentStore>() {}
    assert_doc_store::<JsStorage>();
}

// ── Doc example: "Async Methods — impl styles" (line 153) ───────────

#[allow(dead_code)]
struct MyAsyncImpl;

impl Storage for MyAsyncImpl {
    // Style 1: async fn
    async fn js_save(&self, _key: String, _value: JsValue) -> Result<(), JsValue> {
        Ok(())
    }

    // Style 2: explicit Future return (no extra lifetime bound to avoid
    // refining_impl_trait warning — just use async fn for both in tests)
    async fn js_load(&self, _key: String) -> Result<JsValue, JsValue> {
        Ok(JsValue::UNDEFINED)
    }

    fn js_name(&self) -> String {
        "test".into()
    }
}

#[test]
fn both_async_impl_styles_compile() {
    fn assert_storage<T: Storage>() {}
    assert_storage::<MyAsyncImpl>();
}

// ── Doc example: "Error Types" (line 179) ───────────────────────────

#[js_trait(js_type = JsApi)]
pub trait Api {
    #[wasm_bindgen(js_name = "fetchRaw")]
    async fn js_fetch_raw(&self) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = "fetchTyped")]
    async fn js_fetch_typed(&self) -> Result<js_sys::Uint8Array, js_sys::Error>;

    #[wasm_bindgen(js_name = "send")]
    async fn js_send(&self, data: js_sys::Uint8Array) -> Result<(), js_sys::Error>;
}

#[test]
fn typed_error_api_compiles() {
    fn assert_api<T: Api>() {}
    fn assert_js_cast<T: wasm_bindgen::JsCast>() {}

    assert_api::<JsApi>();
    assert_js_cast::<JsApi>();
}
