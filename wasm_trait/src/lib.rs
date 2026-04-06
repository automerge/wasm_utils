//! JS duck-typed interfaces as Rust traits with compile-time conformance checking.
//!
//! This crate provides two attribute macros:
//!
//! - [`js_trait`] — placed on a Rust trait, generates an `extern "C"` block,
//!   the Rust trait, an `impl Trait for ExternType`, and a TypeScript interface.
//!
//! - [`wasm_implements`] — placed on a `#[wasm_bindgen]` impl block, generates a
//!   compile-time witness that the impl block's methods satisfy a trait, plus a
//!   runtime tag for duck-type conformance checking.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemImpl, ItemTrait, Path};

mod js_trait;
mod shared;
mod wasm_implements;

/// Compile-time check that a `#[wasm_bindgen]` impl block conforms to a trait.
///
/// Generates a hidden trait impl witness (for compile-time checking) and a
/// runtime tag method (for duck-type conformance checking from JS).
///
/// # Example
///
/// ```ignore
/// #[wasm_implements(Transport)]
/// #[wasm_bindgen(js_class = "SubductionHttpLongPoll")]
/// impl WasmHttpLongPoll {
///     #[wasm_bindgen(js_name = "sendBytes")]
///     pub async fn js_send_bytes(&self, bytes: Uint8Array) -> Result<(), JsValue> { ... }
/// }
/// ```
/// Define a JS duck-typed interface as a Rust trait.
///
/// Generates an `extern "C"` block, a Rust trait, an `impl Trait for ExternType`,
/// and a TypeScript interface declaration.
///
/// # Example
///
/// ```ignore
/// #[js_trait(js_type = JsTransport)]
/// pub trait Transport {
///     #[wasm_bindgen(js_name = "sendBytes")]
///     async fn js_send_bytes(&self, bytes: Uint8Array) -> Result<(), JsValue>;
///
///     #[wasm_bindgen(js_name = "recvBytes")]
///     async fn js_recv_bytes(&self) -> Result<Uint8Array, JsValue>;
/// }
/// ```
#[proc_macro_attribute]
pub fn js_trait(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as js_trait::JsTraitArgs);
    let trait_def = parse_macro_input!(item as ItemTrait);
    js_trait::js_trait_impl(args, trait_def).into()
}

/// Compile-time check that a `#[wasm_bindgen]` impl block conforms to a trait.
///
/// Generates a hidden trait impl witness (for compile-time checking) and a
/// runtime tag method (for duck-type conformance checking from JS).
///
/// # Example
///
/// ```ignore
/// #[wasm_implements(Transport)]
/// #[wasm_bindgen(js_class = "SubductionHttpLongPoll")]
/// impl WasmHttpLongPoll {
///     #[wasm_bindgen(js_name = "sendBytes")]
///     pub async fn js_send_bytes(&self, bytes: Uint8Array) -> Result<(), JsValue> { ... }
/// }
/// ```
#[proc_macro_attribute]
pub fn wasm_implements(attr: TokenStream, item: TokenStream) -> TokenStream {
    let trait_path = parse_macro_input!(attr as Path);
    let impl_block = parse_macro_input!(item as ItemImpl);
    wasm_implements::wasm_implements_impl(trait_path, impl_block).into()
}
