//! JS duck-typed interfaces as Rust traits with compile-time conformance checking.
//!
//! This crate provides two attribute macros for bridging JS interfaces into Rust:
//!
//! - [`js_trait`] — Define a JS interface as a Rust trait. Generates an
//!   `extern "C"` block, a TypeScript interface, and an `impl Trait for ExternType`.
//!
//! - [`wasm_implements`] — Verify at compile time that a Rust-exported struct
//!   implements a JS interface. Also injects a runtime tag for duck-type checking.
//!
//! # Quick Start
//!
//! ## 1. Define a JS interface
//!
//! ```ignore
//! use wasm_trait::js_trait;
//!
//! #[js_trait(js_type = JsStorage)]
//! pub trait Storage {
//!     #[wasm_bindgen(js_name = "save")]
//!     async fn js_save(&self, key: String, value: JsValue) -> Result<(), JsValue>;
//!
//!     #[wasm_bindgen(js_name = "load")]
//!     async fn js_load(&self, key: String) -> Result<JsValue, JsValue>;
//!
//!     #[wasm_bindgen(js_name = "name")]
//!     fn js_name(&self) -> String;
//! }
//! ```
//!
//! This generates:
//! - A TypeScript interface `Storage` (for JS consumers)
//! - An extern type `JsStorage` (for receiving JS objects in Rust)
//! - A Rust trait `Storage` (for generic programming)
//! - `impl Storage for JsStorage` (so JS objects satisfy the trait)
//!
//! ## 2. Use the JS object through the trait
//!
//! ```ignore
//! // Accept any JS object that conforms to the interface:
//! fn accept_storage(s: &JsStorage) {
//!     let name = s.js_name();  // works via the Storage trait
//! }
//!
//! // Or use the trait generically:
//! async fn use_storage(s: &impl Storage) {
//!     s.js_save("key".into(), JsValue::from_str("value"))
//!         .await
//!         .expect("save failed");
//! }
//! ```
//!
//! ## 3. Export a Rust struct that implements the same interface
//!
//! ```ignore
//! use wasm_trait::wasm_implements;
//!
//! #[wasm_bindgen]
//! pub struct WasmMemoryStorage { /* ... */ }
//!
//! #[wasm_implements(Storage)]
//! #[wasm_bindgen(js_class = "WasmMemoryStorage")]
//! impl WasmMemoryStorage {
//!     #[wasm_bindgen(js_name = "save")]
//!     pub async fn js_save(&self, key: String, value: JsValue) -> Result<(), JsValue> {
//!         // ...
//!         Ok(())
//!     }
//!
//!     #[wasm_bindgen(js_name = "load")]
//!     pub async fn js_load(&self, key: String) -> Result<JsValue, JsValue> {
//!         // ...
//!         Ok(JsValue::UNDEFINED)
//!     }
//!
//!     #[wasm_bindgen(js_name = "name")]
//!     pub fn js_name(&self) -> String {
//!         "memory".into()
//!     }
//! }
//! ```
//!
//! If any method is missing or has the wrong signature, the compiler catches it.
//!
//! # Bridging to Domain Traits
//!
//! The generated trait uses JS-boundary types (`JsValue`, `Uint8Array`, etc.).
//! In a real application you'll bridge this to a domain trait with proper
//! Rust types. The pattern is:
//!
//! ```ignore
//! // 1. Define the domain trait (in your core crate, no JS dependency)
//! trait DocumentStore {
//!     async fn save(&self, id: &str, content: &[u8]) -> Result<(), MyError>;
//!     async fn load(&self, id: &str) -> Result<Vec<u8>, MyError>;
//! }
//!
//! // 2. Bridge the JS extern type to the domain trait
//! impl DocumentStore for JsStorage {
//!     async fn save(&self, id: &str, content: &[u8]) -> Result<(), MyError> {
//!         // Convert Rust types → JS types
//!         let js_value = Uint8Array::from(content).into();
//!
//!         // Call the js_trait-generated method
//!         self.js_save(id.into(), js_value)
//!             .await
//!             .map_err(MyError::from_js)
//!     }
//!
//!     async fn load(&self, id: &str) -> Result<Vec<u8>, MyError> {
//!         let result = self.js_load(id.into())
//!             .await
//!             .map_err(MyError::from_js)?;
//!
//!         // Convert JS types → Rust types
//!         let array = Uint8Array::new(&result);
//!         Ok(array.to_vec())
//!     }
//! }
//!
//! // 3. Your error type bridges JsValue → typed error
//! struct MyError(String);
//!
//! impl MyError {
//!     fn from_js(val: JsValue) -> Self {
//!         Self(val.as_string().unwrap_or_else(|| "unknown error".into()))
//!     }
//! }
//! ```
//!
//! The bridge is where:
//! - `&str` / `&[u8]` → `JsValue` / `Uint8Array` argument conversion happens
//! - `JsValue` → `Vec<u8>` / domain types return conversion happens
//! - `JsValue` errors → typed `MyError` mapping happens
//!
//! The `js_` prefix on trait methods is intentional — it signals that you're
//! at the JS boundary layer. The domain trait (without the prefix) is where
//! you work with proper Rust types.
//!
//! # Async Methods
//!
//! Trait methods marked `async` must return `Result<T, E>`, because JS
//! promises can reject. The generated extern fn returns `js_sys::Promise`,
//! and the generated impl wraps it with `JsFuture`, converting both the
//! Ok and Err values via `JsCast::unchecked_into()`.
//!
//! For `Result<(), E>`, the Ok value is discarded (JS `Promise<void>`
//! resolves with `undefined`).
//!
//! The generated trait uses `async fn` directly (stable since Rust 1.75).
//! Implementors can use either `async fn` or return `impl Future`:
//!
//! ```ignore
//! // Either style works:
//! impl Storage for MyType {
//!     async fn js_save(&self, key: String, value: JsValue) -> Result<(), JsValue> {
//!         Ok(())
//!     }
//! }
//!
//! // Or with explicit Future:
//! impl Storage for MyType {
//!     fn js_save(&self, key: String, value: JsValue)
//!         -> impl Future<Output = Result<(), JsValue>> + '_
//!     {
//!         async move { Ok(()) }
//!     }
//! }
//! ```
//!
//! # Error Types
//!
//! Both `T` and `E` in `Result<T, E>` can be any type that implements
//! `JsCast` — not just `JsValue`. The generated impl uses
//! `unchecked_into()` to convert the raw `JsValue` from `JsFuture` into
//! the declared types. Choose the type based on what the JS side actually
//! produces:
//!
//! ```ignore
//! #[js_trait(js_type = JsApi)]
//! pub trait Api {
//!     // Error type is JsValue (JS can reject with anything):
//!     #[wasm_bindgen(js_name = "fetchRaw")]
//!     async fn js_fetch_raw(&self) -> Result<JsValue, JsValue>;
//!
//!     // Error type is js_sys::Error (JS always throws Error objects):
//!     #[wasm_bindgen(js_name = "fetchTyped")]
//!     async fn js_fetch_typed(&self) -> Result<Uint8Array, js_sys::Error>;
//!
//!     // Ok type is () for void promises, error is still typed:
//!     #[wasm_bindgen(js_name = "send")]
//!     async fn js_send(&self, data: Uint8Array) -> Result<(), js_sys::Error>;
//! }
//! ```
//!
//! > **Note:** TypeScript does not have typed exceptions, so the error type
//! > `E` does not appear in the generated TypeScript interface.
//! > `Result<Uint8Array, js_sys::Error>` in async maps to
//! > `Promise<Uint8Array>` in TS — the rejection type is implicit.
//!
//! # JS Method Name Checking
//!
//! `#[js_trait]` generates a hidden constant listing the expected JS method
//! names (from `#[wasm_bindgen(js_name = "...")]` attributes). When you use
//! `#[wasm_implements]`, it checks at compile time that the impl block
//! exports all the required JS method names.
//!
//! If a method is missing or has the wrong `js_name`, you get a compile error:
//!
//! ```text
//! error: impl block is missing one or more JS methods required by the interface
//!        (check js_name attrs)
//! ```
//!
//! > **Note:** `#[js_trait]` requires `#[wasm_bindgen(js_name = "...")]` on
//! > every method — omitting it is a compile error. On the
//! > `#[wasm_implements]` side, methods without `js_name` use their Rust
//! > name, which will not match the expected interface — catching the
//! > mismatch at compile time.
//!
//! # Macro Parameters
//!
//! ## `#[js_trait]`
//!
//! | Parameter | Required | Default    | Description                         |
//! |-----------|----------|------------|-------------------------------------|
//! | `js_type` | Yes      | —          | Rust ident for the generated extern type |
//! | `js_name` | No       | Trait name | JS/TS interface name                |
//!
//! ## `#[wasm_implements]`
//!
//! Takes one argument: the trait path to check conformance against.
//!
//! The annotated `impl` block should contain _only_ the methods required
//! by the trait — just like a native `impl Trait for Type` block in Rust.
//! Extra methods (constructors, helpers, etc.) should go in a separate
//! `impl` block. Similarly, use separate `impl` blocks for multiple
//! traits (one per trait).
//!
//! # User Dependencies
//!
//! The generated code references these crates. Your crate must depend on them:
//!
//! - `wasm-bindgen` — always required
//! - `js-sys` — always required
//! - `wasm-bindgen-futures` — required if any trait methods are `async`

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemImpl, ItemTrait, Path};

mod js_trait;
mod shared;
mod wasm_implements;

/// Define a JS duck-typed interface as a Rust trait.
///
/// Generates:
/// 1. A `typescript_custom_section` with the TypeScript interface
/// 2. An `extern "C"` block with `#[wasm_bindgen]` bindings
/// 3. The Rust trait (with `#[wasm_bindgen]` attrs stripped)
/// 4. An `impl Trait for ExternType` that delegates to the extern fns
///
/// See the [module-level documentation](crate) for usage examples.
#[proc_macro_attribute]
pub fn js_trait(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as js_trait::JsTraitArgs);
    let trait_def = parse_macro_input!(item as ItemTrait);
    js_trait::js_trait_impl(args, &trait_def).into()
}

/// Compile-time check that a `#[wasm_bindgen]` impl block conforms to a trait.
///
/// Generates a hidden trait impl witness (catches missing or mistyped methods
/// at compile time) and a runtime tag method for duck-type conformance
/// checking from JS.
///
/// Use separate `impl` blocks when checking conformance against multiple
/// traits — one `#[wasm_implements(Trait)]` per block, mirroring how native
/// Rust trait impls work.
///
/// See the [module-level documentation](crate) for usage examples.
#[proc_macro_attribute]
pub fn wasm_implements(attr: TokenStream, item: TokenStream) -> TokenStream {
    let trait_path = parse_macro_input!(attr as Path);
    let impl_block = parse_macro_input!(item as ItemImpl);
    wasm_implements::wasm_implements_impl(&trait_path, impl_block).into()
}
