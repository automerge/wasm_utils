//! Realistic downstream-developer scenario: a document sync system.
//!
//! This module simulates what a developer building a CRDT-based collaborative
//! editor would experience when using `wasm_trait`. It exercises:
//!
//! - Multiple `#[js_trait]` definitions (async + sync)
//! - `#[wasm_implements]` for Rust-side in-memory storage
//! - Generic functions bounded by the generated traits
//! - Bridging to a "core" Rust trait with richer types
//! - Awkward patterns and DX observations

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
use wasm_trait::{js_trait, wasm_implements};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 1. Document Storage — async interface (the main "repository" abstraction)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 2. Auth — sync interface (session/permission checks)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[js_trait(js_type = JsAuth)]
pub trait Auth {
    #[wasm_bindgen(js_name = "currentUser")]
    fn js_current_user(&self) -> JsValue;

    #[wasm_bindgen(js_name = "hasPermission")]
    fn js_has_permission(&self, resource: JsValue, action: JsValue) -> bool;
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 3. Rust-exported in-memory storage implementing DocStorage
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 4. Generic functions bounded by the generated traits
//
// DX observation: This is the primary payoff — you can write Rust code that
// is generic over *both* a JS-provided implementation and a Rust-native one.
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Generic function that accepts any DocStorage implementation.
///
/// DX: Works smoothly. The trait bound reads naturally.
/// The method names (`js_save_document`, etc.) are the only friction —
/// they leak the JS-binding concern into pure Rust generic code.
fn save_if_permitted<S: DocStorage, A: Auth>(
    _storage: &S,
    _auth: &A,
    _doc_id: JsValue,
    _content: JsValue,
) {
    // In real code you'd check auth then call storage.
    // Here we just verify the bounds compile.
    let _user = _auth.js_current_user();
    let _allowed = _auth.js_has_permission(JsValue::NULL, JsValue::NULL);

    // AWKWARD: Can't call async methods in a sync context, but the
    // _type bounds_ still compile. This is correct behavior — async
    // methods return impl Future, not a value — but a new user might
    // expect to be able to call .await here without an async fn wrapper.
}

/// Async generic function — the natural way to use async trait methods.
///
/// DX: Works well. `impl Future` return type from RPITIT means `.await`
/// works as expected inside an async block.
async fn load_document_generic<S: DocStorage>(
    storage: &S,
    id: JsValue,
) -> Result<JsValue, JsValue> {
    storage.js_load_document(id).await
}

/// Combining multiple trait bounds.
///
/// DX: Standard Rust patterns work. `S: DocStorage + Auth` would not make
/// sense here (different concerns), but `where` clauses with multiple
/// generics work fine.
async fn authorized_delete<S: DocStorage, A: Auth>(
    storage: &S,
    auth: &A,
    doc_id: JsValue,
) -> Result<(), JsValue> {
    let _has_perm = auth.js_has_permission(JsValue::NULL, JsValue::NULL);
    storage.js_delete_document(doc_id).await
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 5. Bridging to a "core" Rust trait with richer types
//
// In a real app, you'd want a domain trait with proper types (String, Vec<u8>,
// domain structs) rather than JsValue everywhere. This section shows the
// adapter/bridge pattern.
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// The "pure Rust" domain trait — no JS types, no wasm_bindgen.
trait CoreDocStorage {
    fn save(&self, id: &str, content: &[u8]) -> Result<(), String>;
    fn load(&self, id: &str) -> Result<Vec<u8>, String>;
    fn list(&self) -> Result<Vec<String>, String>;
    fn delete(&self, id: &str) -> Result<(), String>;
}

/// Bridge: implement the core trait for the extern JS type by converting
/// JS values.
///
/// DX OBSERVATION — MAJOR AWKWARDNESS:
/// We cannot implement CoreDocStorage for JsDocStorage directly because
/// CoreDocStorage has async-incompatible signatures (returns `Result`, not
/// `impl Future<Output = Result<...>>`), and the generated DocStorage trait
/// methods are async (return `impl Future`).
///
/// In practice you'd need either:
/// (a) Make CoreDocStorage async too (using async-trait or RPITIT)
/// (b) Use a blocking bridge (not possible in Wasm)
/// (c) Give up on bridging and use the JS-typed trait directly
///
/// For demonstration, we show the sync-to-sync bridge for Auth, which
/// works naturally:

/// Sync bridge works: Auth → CoreAuth
trait CoreAuth {
    fn current_user(&self) -> String;
    fn has_permission(&self, resource: &str, action: &str) -> bool;
}

/// Implementing a core Rust trait for the JS extern type.
///
/// DX: This works, but the conversion from JsValue to Rust types is manual
/// and verbose. You have to call `.as_string().unwrap_or_default()` etc.
/// for every value. There's no automatic serde bridge.
impl CoreAuth for JsAuth {
    fn current_user(&self) -> String {
        // Delegates to the generated trait method, then converts
        self.js_current_user().as_string().unwrap_or_default()
    }

    fn has_permission(&self, resource: &str, action: &str) -> bool {
        // Must convert &str → JsValue for the generated method
        self.js_has_permission(JsValue::from_str(resource), JsValue::from_str(action))
    }
}

/// Generic code using the pure Rust trait.
fn check_access_core<A: CoreAuth>(auth: &A, resource: &str) -> bool {
    let user = auth.current_user();
    if user.is_empty() {
        return false;
    }
    auth.has_permission(resource, "read")
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 6. Awkward patterns and DX observations
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

// --- 6a. Method naming: `js_` prefix leaks into generic code ---
//
// When you write generic functions, you end up calling `storage.js_save_document()`
// rather than `storage.save_document()`. The `js_` prefix is a wasm_bindgen
// naming convention to avoid Rust keyword collisions, but it pollutes the
// Rust API surface.
//
// Suggestion: Support an optional `rust_name` parameter on trait methods to
// generate a separate Rust-facing name, e.g.:
//   #[wasm_bindgen(js_name = "saveDocument", rust_name = "save_document")]
//
// Or auto-strip `js_` prefix from the trait method name.

// --- 6b. All parameters are JsValue — no type safety at the Rust level ---
//
// Because wasm_bindgen extern types use JsValue for opaque JS objects,
// the generated trait methods accept and return JsValue. This means the
// Rust compiler can't catch a caller passing a document ID where content
// is expected.
//
// This is inherent to the JS interop boundary, not a bug in wasm_trait.
// But it would be useful to document the "bridge pattern" (section 5) as
// a recommended practice.

// --- 6c. Error types are always JsValue ---
//
// There's no way to use a custom Rust error type in the trait definition.
// Async methods must return Result<T, JsValue>. This is correct (JS
// promises reject with arbitrary values) but makes error handling in
// generic Rust code awkward — you can't match on error variants.

// --- 6d. No trait inheritance / supertraits across js_traits ---
//
// You can't write:
//   #[js_trait(js_type = JsVersionedStorage)]
//   pub trait VersionedStorage: DocStorage { ... }
//
// Each js_trait is independent. If you want a combined interface, you
// need a separate trait that bounds on both:
//   trait VersionedDocStorage: DocStorage + VersionedStorage {}
//
// This is a standard Rust pattern, but the macro doesn't help compose
// interfaces. It's not clear if supertrait syntax would even work.

// --- 6e. wasm_implements requires separate impl blocks per trait ---
//
// If a struct implements both DocStorage and Auth, you need two separate
// `impl` blocks with separate `#[wasm_implements]` attrs. This is
// documented in qa_edge_cases but might surprise newcomers.

// --- 6f. No way to make a "mock" for testing without wasm_bindgen ---
//
// In native tests, you can't easily create a mock that implements
// DocStorage because the trait methods return `impl Future + '_`
// (RPITIT), which makes it hard to implement outside of the macro's
// generated code. Manual impl requires matching the exact RPITIT form.
//
// Actually — let's TRY to implement DocStorage manually for a mock type
// and see what happens:

struct MockDocStorage;

impl DocStorage for MockDocStorage {
    fn js_save_document(
        &self,
        _id: JsValue,
        _content: JsValue,
    ) -> impl core::future::Future<Output = Result<(), JsValue>> + '_ {
        async { Ok(()) }
    }

    fn js_load_document(
        &self,
        _id: JsValue,
    ) -> impl core::future::Future<Output = Result<JsValue, JsValue>> + '_ {
        async { Ok(JsValue::NULL) }
    }

    fn js_list_documents(
        &self,
    ) -> impl core::future::Future<Output = Result<js_sys::Array, JsValue>> + '_ {
        async { Ok(js_sys::Array::new()) }
    }

    fn js_delete_document(
        &self,
        _id: JsValue,
    ) -> impl core::future::Future<Output = Result<(), JsValue>> + '_ {
        async { Ok(()) }
    }
}

// DX VERDICT on 6f: Manual impl DOES work! The RPITIT approach means you
// can implement the trait with `-> impl Future<Output = ...> + '_` and
// return an `async` block. This is much better than if the macro had used
// `#[async_trait]` which would require `Pin<Box<dyn Future>>`.
//
// However, the ergonomics of writing out the full RPITIT signature are
// verbose. A helper macro like `#[wasm_mock(DocStorage)]` could generate
// a mock skeleton.

// --- 6g. Sync mock is straightforward ---

struct MockAuth;

impl Auth for MockAuth {
    fn js_current_user(&self) -> JsValue {
        JsValue::from_str("test-user")
    }

    fn js_has_permission(&self, _resource: JsValue, _action: JsValue) -> bool {
        true
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

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
    // DX FINDING: We can't actually *call* save_if_permitted with mocks on
    // native targets because MockAuth::js_current_user returns JsValue::from_str(),
    // which panics on non-wasm32. This is a fundamental limitation: even pure-Rust
    // mock impls of js_trait-generated traits are infected by JsValue, making them
    // useless for native unit tests.
    //
    // Verify type compatibility only:
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
