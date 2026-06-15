//! Realistic downstream scenario: a document-sync system.
//!
//! Compile-tests that real-world patterns work together: multiple
//! `#[js_trait]` definitions (async + sync), `#[wasm_implements]` for a
//! Rust-exported storage type, generic functions bounded by the generated
//! traits, and bridging a generated trait to a pure-Rust domain trait.

#![allow(
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value,
    clippy::unused_self,
    dead_code,
    missing_docs,
    unreachable_pub
)]

use wasm_bindgen::prelude::*;
use wasm_bindgen_trait::{js_trait, wasm_implements};

// Document storage: async interface.
#[js_trait(js_type = JsDocStorage)]
pub trait DocStorage {
    #[wasm_bindgen(js_name = "saveDocument")]
    async fn js_save_document(&self, id: JsValue, content: JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = "loadDocument")]
    async fn js_load_document(&self, id: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = "listDocuments")]
    async fn js_list_documents(&self) -> Result<js_sys::Array, JsValue>;

    #[wasm_bindgen(js_name = "deleteDocument")]
    async fn js_delete_document(&self, id: JsValue) -> Result<(), JsValue>;
}

// Auth: sync interface.
#[js_trait(js_type = JsAuth)]
pub trait Auth {
    #[wasm_bindgen(js_name = "currentUser")]
    fn js_current_user(&self) -> JsValue;

    #[wasm_bindgen(js_name = "hasPermission")]
    fn js_has_permission(&self, resource: JsValue, action: JsValue) -> bool;
}

// Rust-exported in-memory storage implementing DocStorage.
#[wasm_bindgen]
pub struct WasmInMemoryDocStorage;

#[wasm_implements(DocStorage)]
#[wasm_bindgen(js_class = "WasmInMemoryDocStorage")]
impl WasmInMemoryDocStorage {
    #[wasm_bindgen(js_name = "saveDocument")]
    pub async fn js_save_document(&self, _id: JsValue, _content: JsValue) -> Result<(), JsValue> {
        Ok(())
    }

    #[wasm_bindgen(js_name = "loadDocument")]
    pub async fn js_load_document(&self, _id: JsValue) -> Result<JsValue, JsValue> {
        Ok(JsValue::NULL)
    }

    #[wasm_bindgen(js_name = "listDocuments")]
    pub async fn js_list_documents(&self) -> Result<js_sys::Array, JsValue> {
        Ok(js_sys::Array::new())
    }

    #[wasm_bindgen(js_name = "deleteDocument")]
    pub async fn js_delete_document(&self, _id: JsValue) -> Result<(), JsValue> {
        Ok(())
    }
}

// Generic functions bounded by the generated traits.

fn save_if_permitted<S: DocStorage, A: Auth>(
    _storage: &S,
    _auth: &A,
    _doc_id: JsValue,
    _content: JsValue,
) {
    let _user = _auth.js_current_user();
    let _allowed = _auth.js_has_permission(JsValue::NULL, JsValue::NULL);
}

async fn load_document_generic<S: DocStorage>(
    storage: &S,
    id: JsValue,
) -> Result<JsValue, JsValue> {
    storage.js_load_document(id).await
}

async fn authorized_delete<S: DocStorage, A: Auth>(
    storage: &S,
    auth: &A,
    doc_id: JsValue,
) -> Result<(), JsValue> {
    let _has_perm = auth.js_has_permission(JsValue::NULL, JsValue::NULL);
    storage.js_delete_document(doc_id).await
}

// Bridging a generated trait to a pure-Rust domain trait (no JS types).
trait CoreAuth {
    fn current_user(&self) -> String;
    fn has_permission(&self, resource: &str, action: &str) -> bool;
}

impl CoreAuth for JsAuth {
    fn current_user(&self) -> String {
        self.js_current_user().as_string().unwrap_or_default()
    }

    fn has_permission(&self, resource: &str, action: &str) -> bool {
        self.js_has_permission(JsValue::from_str(resource), JsValue::from_str(action))
    }
}

fn check_access_core<A: CoreAuth>(auth: &A, resource: &str) -> bool {
    let user = auth.current_user();
    if user.is_empty() {
        return false;
    }
    auth.has_permission(resource, "read")
}

// Pure-Rust mocks. Generated traits use `async fn` directly, so mocks need no
// RPITIT or `#[async_trait]` boilerplate.
struct MockDocStorage;

impl DocStorage for MockDocStorage {
    async fn js_save_document(&self, _id: JsValue, _content: JsValue) -> Result<(), JsValue> {
        Ok(())
    }

    async fn js_load_document(&self, _id: JsValue) -> Result<JsValue, JsValue> {
        Ok(JsValue::NULL)
    }

    async fn js_list_documents(&self) -> Result<js_sys::Array, JsValue> {
        Ok(js_sys::Array::new())
    }

    async fn js_delete_document(&self, _id: JsValue) -> Result<(), JsValue> {
        Ok(())
    }
}

struct MockAuth;

impl Auth for MockAuth {
    fn js_current_user(&self) -> JsValue {
        JsValue::from_str("test-user")
    }

    fn js_has_permission(&self, _resource: JsValue, _action: JsValue) -> bool {
        true
    }
}

// Tests

#[test]
fn extern_types_implement_traits() {
    fn assert_doc_storage<T: DocStorage>() {}
    fn assert_auth<T: Auth>() {}

    assert_doc_storage::<JsDocStorage>();
    assert_auth::<JsAuth>();
}

#[test]
fn rust_export_implements_doc_storage() {
    fn assert_doc_storage<T: DocStorage>() {}
    assert_doc_storage::<WasmInMemoryDocStorage>();
}

#[test]
fn generic_function_accepts_both_impls() {
    fn use_storage<T: DocStorage>(_s: &T) {}
    let _ = use_storage::<JsDocStorage>;
    let _ = use_storage::<WasmInMemoryDocStorage>;
}

#[test]
fn multi_trait_generic_compiles() {
    fn use_both<S: DocStorage, A: Auth>(_s: &S, _a: &A) {}
    let _ = use_both::<JsDocStorage, JsAuth>;
    let _ = use_both::<WasmInMemoryDocStorage, JsAuth>;
}

#[test]
fn mock_implements_doc_storage() {
    fn assert_doc_storage<T: DocStorage>() {}
    assert_doc_storage::<MockDocStorage>();
}

#[test]
fn mock_implements_auth() {
    fn assert_auth<T: Auth>() {}
    assert_auth::<MockAuth>();
}

#[test]
fn mock_usable_in_generic_functions() {
    // Type-compatibility only: `JsValue::from_str` panics on non-wasm32, so
    // these can't be invoked on the host.
    let _ = save_if_permitted::<MockDocStorage, MockAuth>;
    let _ = check_access_core::<JsAuth>;
}

#[test]
fn core_bridge_compiles_for_extern_type() {
    fn assert_core_auth<T: CoreAuth>() {}
    assert_core_auth::<JsAuth>();
}

#[test]
fn async_generic_function_compiles() {
    fn check_types<S: DocStorage>() {
        fn assert_future<T: core::future::Future>(_t: &T) {}
        fn inner<S: DocStorage>(s: &S) {
            let fut = load_document_generic(s, JsValue::NULL);
            assert_future(&fut);
        }
        let _ = inner::<S>;
    }
    check_types::<JsDocStorage>();
    check_types::<WasmInMemoryDocStorage>();
    check_types::<MockDocStorage>();
}

#[test]
fn multi_bound_async_generic_compiles() {
    fn check_types<S: DocStorage, A: Auth>() {
        fn inner<S: DocStorage, A: Auth>(s: &S, a: &A) {
            let _fut = authorized_delete(s, a, JsValue::NULL);
        }
        let _ = inner::<S, A>;
    }
    check_types::<JsDocStorage, JsAuth>();
    check_types::<WasmInMemoryDocStorage, MockAuth>();
    check_types::<MockDocStorage, MockAuth>();
}
