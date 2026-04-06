//! Naming conventions for generated code.

use alloc::{
    format,
    string::{String, ToString},
};
use heck::ToSnakeCase;
use proc_macro2::Span;
use syn::Ident;

/// Generate the Rust ident for the runtime tag method.
///
/// Given trait name `Transport`, returns `__wasm_impl_transport`.
pub(crate) fn tag_method_rust_ident(trait_name: &Ident) -> Ident {
    let snake = trait_name.to_string().to_snake_case();
    Ident::new(&format!("__wasm_impl_{snake}"), Span::call_site())
}

/// Generate the JS name string for the runtime tag method.
///
/// Given trait name `Transport`, returns `"__wasm_impl_Transport"`.
pub(crate) fn tag_method_js_name(trait_name: &Ident) -> String {
    format!("__wasm_impl_{trait_name}")
}

/// Generate the Rust ident for a `__wasm_trait_`-prefixed extern fn.
///
/// Given method name `js_send_bytes`, returns `__wasm_trait_js_send_bytes`.
pub(crate) fn extern_fn_ident(method_name: &Ident) -> Ident {
    Ident::new(&format!("__wasm_trait_{method_name}"), method_name.span())
}

/// Generate the Rust ident for the hidden JS interface names const.
///
/// Given trait name `Transport`, returns `__JS_INTERFACE_TRANSPORT`.
pub(crate) fn js_interface_const_ident(trait_name: &Ident) -> Ident {
    use heck::ToShoutySnakeCase;
    let screaming = trait_name.to_string().to_shouty_snake_case();
    Ident::new(&format!("__JS_INTERFACE_{screaming}"), trait_name.span())
}
