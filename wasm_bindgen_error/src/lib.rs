#![cfg_attr(not(feature = "std"), no_std)]
//! Derive macro for converting Rust error types into named JS errors.
//!
//! # Motivation
//!
//! When exposing Rust error types through `wasm-bindgen`, you need a
//! `From<MyError> for JsValue` impl so that `Result<T, MyError>` can cross
//! the Wasm boundary. The typical implementation creates a [`js_sys::Error`]
//! with the error's `Display` message and sets a meaningful `.name` property
//! so that JS consumers can distinguish error types.
//!
//! Writing this by hand is pure boilerplate — the body is always:
//!
//! ```rust,ignore
//! impl From<MyError> for JsValue {
//!     fn from(err: MyError) -> Self {
//!         let js_err = js_sys::Error::new(&err.to_string());
//!         js_err.set_name("MyError");
//!         js_err.into()
//!     }
//! }
//! ```
//!
//! `#[derive(WasmError)]` generates exactly this.
//!
//! # Usage
//!
//! ```rust,ignore
//! use wasm_bindgen_error::WasmError;
//! use thiserror::Error;
//!
//! // JS name defaults to "HydrationError" (strips "Wasm" prefix)
//! #[derive(Debug, Error, WasmError)]
//! #[error(transparent)]
//! pub struct WasmHydrationError(#[from] HydrationError);
//!
//! // Explicit JS name override
//! #[derive(Debug, Error, WasmError)]
//! #[wasm_error(js_name = "StorageFailure")]
//! #[error(transparent)]
//! pub struct WasmIoError(#[from] IoError);
//! ```
//!
//! The derive requires that the type implements [`core::error::Error`]
//! (which implies [`core::fmt::Display`] + [`core::fmt::Debug`]).
//! How you provide that is up to you — `thiserror`, a manual impl, etc.
//!
//! # JS Error Name Resolution
//!
//! 1. If `#[wasm_error(js_name = "...")]` is present, use that string literally.
//! 2. Otherwise, strip a leading `Wasm` prefix from the Rust type name.
//!    - `WasmHydrationError` → `"HydrationError"`
//!    - `ConnectError` → `"ConnectError"` (no prefix to strip)

extern crate alloc;

use alloc::string::{String, ToString};
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, LitStr};

/// Derive `From<Self> for JsValue` that creates a named JS error.
///
/// See the [crate-level documentation](crate) for usage and examples.
#[proc_macro_derive(WasmError, attributes(wasm_error))]
pub fn derive_wasm_error(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match wasm_error_impl(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn wasm_error_impl(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let type_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let js_name = extract_js_name(input)?;

    let js_name_str = js_name.unwrap_or_else(|| strip_wasm_prefix(&type_name.to_string()));

    // Extend any existing where clause with a `Self: ::core::error::Error` bound
    // so we get a clear compile error if the type doesn't implement Error.
    let extended_where = if let Some(wc) = where_clause {
        quote! { #wc #type_name #ty_generics: ::core::error::Error, }
    } else {
        quote! { where #type_name #ty_generics: ::core::error::Error, }
    };

    Ok(quote! {
        impl #impl_generics ::core::convert::From<#type_name #ty_generics> for ::wasm_bindgen::JsValue
        #extended_where
        {
            fn from(err: #type_name #ty_generics) -> Self {
                let js_err = ::js_sys::Error::new(
                    &err.to_string(),
                );
                js_err.set_name(#js_name_str);
                js_err.into()
            }
        }
    })
}

/// Extract `js_name = "..."` from `#[wasm_error(js_name = "...")]` if present.
fn extract_js_name(input: &DeriveInput) -> syn::Result<Option<String>> {
    for attr in &input.attrs {
        if !attr.path().is_ident("wasm_error") {
            continue;
        }

        let mut js_name: Option<String> = None;

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("js_name") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                js_name = Some(lit.value());
                Ok(())
            } else {
                Err(meta.error("unknown attribute, expected `js_name = \"...\"`"))
            }
        })?;

        if let Some(name) = js_name {
            return Ok(Some(name));
        }
    }

    Ok(None)
}

/// Strip a leading `Wasm` prefix from a type name.
///
/// `WasmFooError` → `"FooError"`, `FooError` → `"FooError"`.
fn strip_wasm_prefix(name: &str) -> String {
    name.strip_prefix("Wasm").unwrap_or(name).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: parse a token stream as a `DeriveInput`, run the macro,
    /// and return the output as a `String`.
    fn expand(input: proc_macro2::TokenStream) -> String {
        let derive_input: DeriveInput = syn::parse2(input).expect("failed to parse derive input");
        wasm_error_impl(&derive_input)
            .expect("macro expansion failed")
            .to_string()
    }

    /// Helper: parse a token stream as a `DeriveInput`, run the macro,
    /// and return the error message as a `String`.
    fn expand_err(input: proc_macro2::TokenStream) -> String {
        let derive_input: DeriveInput = syn::parse2(input).expect("failed to parse derive input");
        wasm_error_impl(&derive_input)
            .expect_err("expected macro expansion to fail")
            .to_string()
    }

    #[test]
    fn struct_strips_wasm_prefix() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct WasmHydrationError(HydrationError);
        });

        assert!(
            output.contains("From < WasmHydrationError > for :: wasm_bindgen :: JsValue")
                || output.contains("From<WasmHydrationError> for ::wasm_bindgen::JsValue"),
            "must generate From<WasmHydrationError> for JsValue.\nOutput: {output}",
        );
        assert!(
            output.contains(r#"set_name ("HydrationError")"#)
                || output.contains(r#"set_name("HydrationError")"#),
            "must strip Wasm prefix for JS error name.\nOutput: {output}",
        );
    }

    #[test]
    fn struct_without_wasm_prefix() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct ConnectError;
        });

        assert!(
            output.contains(r#"set_name ("ConnectError")"#)
                || output.contains(r#"set_name("ConnectError")"#),
            "must use type name as-is when no Wasm prefix.\nOutput: {output}",
        );
    }

    #[test]
    fn enum_generates_from_impl() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub enum WasmConnectError {
                Transport(TransportError),
                Handshake(HandshakeError),
            }
        });

        assert!(
            output.contains("From < WasmConnectError > for :: wasm_bindgen :: JsValue")
                || output.contains("From<WasmConnectError> for ::wasm_bindgen::JsValue"),
            "must generate From impl for enums.\nOutput: {output}",
        );
        assert!(
            output.contains(r#"set_name ("ConnectError")"#)
                || output.contains(r#"set_name("ConnectError")"#),
            "must strip Wasm prefix for enum.\nOutput: {output}",
        );
    }

    #[test]
    fn js_name_override() {
        let output = expand(quote! {
            #[derive(Debug)]
            #[wasm_error(js_name = "StorageFailure")]
            pub struct WasmIoError(IoError);
        });

        assert!(
            output.contains(r#"set_name ("StorageFailure")"#)
                || output.contains(r#"set_name("StorageFailure")"#),
            "must use explicit js_name override.\nOutput: {output}",
        );
    }

    #[test]
    fn uses_fully_qualified_paths() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct WasmFooError;
        });

        assert!(
            output.contains(":: core :: convert :: From")
                || output.contains("::core::convert::From"),
            "From impl must use fully qualified ::core::convert::From.\nOutput: {output}",
        );
        assert!(
            output.contains(":: wasm_bindgen :: JsValue")
                || output.contains("::wasm_bindgen::JsValue"),
            "must use fully qualified ::wasm_bindgen::JsValue.\nOutput: {output}",
        );
        assert!(
            output.contains(":: js_sys :: Error :: new") || output.contains("::js_sys::Error::new"),
            "must use fully qualified ::js_sys::Error::new.\nOutput: {output}",
        );
        assert!(
            output.contains("to_string"),
            "must call .to_string() on the error.\nOutput: {output}",
        );
        assert!(
            output.contains(":: core :: error :: Error") || output.contains("::core::error::Error"),
            "must require ::core::error::Error bound.\nOutput: {output}",
        );
    }

    #[test]
    fn requires_error_bound() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct WasmFooError;
        });

        assert!(
            output.contains("where WasmFooError : :: core :: error :: Error")
                || output.contains("where WasmFooError: ::core::error::Error")
                || output.contains("where WasmFooError : ::core::error::Error"),
            "must generate where clause requiring core::error::Error.\nOutput: {output}",
        );
    }

    #[test]
    fn strip_wasm_prefix_with_prefix() {
        assert_eq!(strip_wasm_prefix("WasmFooError"), "FooError");
    }

    #[test]
    fn strip_wasm_prefix_without_prefix() {
        assert_eq!(strip_wasm_prefix("ConnectError"), "ConnectError");
    }

    #[test]
    fn strip_wasm_prefix_exact_wasm() {
        assert_eq!(strip_wasm_prefix("Wasm"), "");
    }

    #[test]
    fn generics_are_preserved() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct WasmError<T>(T);
        });

        assert!(
            output.contains("< T >") || output.contains("<T>"),
            "generic parameters must appear in the impl.\nOutput: {output}",
        );
    }

    // ── Naming convention tests ───────────────────────────────────────

    #[test]
    fn strip_wasm_prefix_lowercase_wasm_unchanged() {
        assert_eq!(strip_wasm_prefix("wasmError"), "wasmError");
    }

    #[test]
    fn strip_wasm_prefix_only_strips_leading() {
        assert_eq!(strip_wasm_prefix("MyWasmError"), "MyWasmError");
    }

    #[test]
    fn js_name_override_on_enum() {
        let output = expand(quote! {
            #[derive(Debug)]
            #[wasm_error(js_name = "CustomEnumError")]
            pub enum WasmMultiError {
                A,
                B,
            }
        });

        assert!(
            output.contains(r#"set_name ("CustomEnumError")"#)
                || output.contains(r#"set_name("CustomEnumError")"#),
            "js_name override must work on enums.\nOutput: {output}",
        );
    }

    // ── Struct shape variations ─────────────────────────────────────

    #[test]
    fn unit_struct() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct WasmUnitError;
        });

        assert!(
            output.contains(r#"set_name ("UnitError")"#)
                || output.contains(r#"set_name("UnitError")"#),
            "must handle unit structs.\nOutput: {output}",
        );
    }

    #[test]
    fn tuple_struct_single_field() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct WasmWrappedError(InnerError);
        });

        assert!(
            output.contains("From < WasmWrappedError >")
                || output.contains("From<WasmWrappedError>"),
            "must handle single-field tuple structs.\nOutput: {output}",
        );
    }

    #[test]
    fn named_fields_struct() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct WasmDetailedError {
                message: String,
                code: u32,
            }
        });

        assert!(
            output.contains(r#"set_name ("DetailedError")"#)
                || output.contains(r#"set_name("DetailedError")"#),
            "must handle structs with named fields.\nOutput: {output}",
        );
    }

    // ── Enum variations ─────────────────────────────────────────────

    #[test]
    fn enum_with_unit_variants() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub enum WasmSimpleError {
                NotFound,
                PermissionDenied,
                Timeout,
            }
        });

        assert!(
            output.contains(r#"set_name ("SimpleError")"#)
                || output.contains(r#"set_name("SimpleError")"#),
            "must handle enums with unit variants.\nOutput: {output}",
        );
    }

    #[test]
    fn enum_with_mixed_variants() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub enum WasmMixedError {
                Simple,
                WithData(u32),
                WithFields { msg: String },
            }
        });

        assert!(
            output.contains("From < WasmMixedError >") || output.contains("From<WasmMixedError>"),
            "must handle enums with mixed variant kinds.\nOutput: {output}",
        );
    }

    // ── Generics ────────────────────────────────────────────────────

    #[test]
    fn generic_with_trait_bounds() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct WasmGenericError<T: core::fmt::Debug>(T);
        });

        assert!(
            output.contains("core :: fmt :: Debug") || output.contains("core::fmt::Debug"),
            "trait bounds on generics must be preserved.\nOutput: {output}",
        );
        assert!(
            output.contains(":: core :: error :: Error") || output.contains("::core::error::Error"),
            "Error bound must still be present.\nOutput: {output}",
        );
    }

    #[test]
    fn generic_with_lifetime() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct WasmBorrowError<'a>(&'a str);
        });

        assert!(
            output.contains("'a"),
            "lifetime parameters must be preserved.\nOutput: {output}",
        );
    }

    #[test]
    fn multiple_generics() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct WasmPairError<A, B>(A, B);
        });

        let has_both = (output.contains("< A , B >") || output.contains("<A, B>"))
            && (output.contains("WasmPairError < A , B >")
                || output.contains("WasmPairError<A, B>"));

        assert!(
            has_both,
            "multiple generic parameters must all appear.\nOutput: {output}",
        );
    }

    #[test]
    fn generic_error_bound_appended_to_existing_where_clause() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct WasmBoundedError<T> where T: Clone (T);
        });

        assert!(
            output.contains(":: core :: error :: Error") || output.contains("::core::error::Error"),
            "Error bound must be appended even when where clause exists.\nOutput: {output}",
        );
        assert!(
            output.contains("Clone"),
            "existing where clause bounds must be preserved.\nOutput: {output}",
        );
    }

    // ── Attribute interaction ───────────────────────────────────────

    #[test]
    fn other_attributes_are_ignored() {
        let output = expand(quote! {
            #[derive(Debug, Clone)]
            #[allow(dead_code)]
            #[cfg(target_arch = "wasm32")]
            pub struct WasmAnnotatedError(String);
        });

        assert!(
            output.contains(r#"set_name ("AnnotatedError")"#)
                || output.contains(r#"set_name("AnnotatedError")"#),
            "non-wasm_error attributes must not interfere.\nOutput: {output}",
        );
    }

    #[test]
    fn empty_wasm_error_attr_uses_default() {
        let output = expand(quote! {
            #[derive(Debug)]
            #[wasm_error()]
            pub struct WasmEmptyAttrError;
        });

        assert!(
            output.contains(r#"set_name ("EmptyAttrError")"#)
                || output.contains(r#"set_name("EmptyAttrError")"#),
            "empty #[wasm_error()] must fall through to default prefix stripping.\nOutput: {output}",
        );
    }

    // ── Error paths ─────────────────────────────────────────────────

    #[test]
    fn error_on_unknown_attribute_key() {
        let err = expand_err(quote! {
            #[derive(Debug)]
            #[wasm_error(unknown_key = "x")]
            pub struct WasmFooError;
        });

        assert!(
            err.contains("unknown attribute"),
            "must reject unknown attribute keys.\nError: {err}",
        );
    }

    #[test]
    fn error_on_non_string_js_name() {
        let err = expand_err(quote! {
            #[derive(Debug)]
            #[wasm_error(js_name = 42)]
            pub struct WasmFooError;
        });

        assert!(
            !err.is_empty(),
            "must reject non-string literal for js_name.\nError: {err}",
        );
    }

    #[test]
    fn error_on_bare_key_without_value() {
        let err = expand_err(quote! {
            #[derive(Debug)]
            #[wasm_error(js_name)]
            pub struct WasmFooError;
        });

        assert!(
            !err.is_empty(),
            "must reject bare key without `= \"value\"`.\nError: {err}",
        );
    }
}
