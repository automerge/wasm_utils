//! Derive macro for converting Rust error types into named JS errors.
//!
//! # Motivation
//!
//! When exposing Rust error types through `wasm-bindgen`, you need a
//! `From<MyError> for JsValue` impl so that `Result<T, MyError>` can cross
//! the Wasm boundary. The typical implementation creates a
//! [`js_sys::Error`](https://docs.rs/js-sys/latest/js_sys/struct.Error.html)
//! with the error's `Display` message and sets a meaningful `.name` property
//! so that JS consumers can distinguish error types.
//!
//! Writing this by hand is pure boilerplate — the body is always:
//!
//! ```ignore
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
//! ```ignore
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
//! _Compiled and tested in `tests/wasm_error_tests/src/doc_examples.rs`._
//!
//! The derive requires that the type implements [`core::error::Error`]
//! (which implies [`core::fmt::Display`] + [`core::fmt::Debug`]).
//! How you provide that is up to you — `thiserror`, a manual impl, etc.
//!
//! # JS Error Name Resolution
//!
//! 1. If `#[wasm_error(js_name = "...")]` is present, use that string literally.
//! 2. Otherwise, strip a leading `Wasm` prefix from the Rust type name, but
//!    only at a CamelCase word boundary (the next character is uppercase).
//!    - `WasmHydrationError` → `"HydrationError"`
//!    - `ConnectError` → `"ConnectError"` (no prefix to strip)
//!    - `WasmtimeError` → `"WasmtimeError"` (`Wasm` is not a separate word)
//!
//! `js_name` may be given at most once, and `#[wasm_error]` is only accepted
//! on the type itself, not on variants or fields.
//!
//! # Requirements
//!
//! The expansion refers to `::wasm_bindgen` and `::js_sys` by absolute path,
//! so the consuming crate must depend on both `wasm-bindgen` and `js-sys`
//! directly (and must not rename them).

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, parse_quote, Attribute, Data, DeriveInput, Fields, LitStr};

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

    reject_misplaced_attrs(&input.data)?;

    let js_name_str = extract_js_name(&input.attrs)?
        .unwrap_or_else(|| strip_wasm_prefix(&type_name.to_string()).to_owned());

    if js_name_str.trim().is_empty() {
        return Err(syn::Error::new(
            type_name.span(),
            "resolved JS error name is empty — use #[wasm_error(js_name = \"...\")] to specify a name",
        ));
    }

    // Add a `Self: ::core::error::Error` bound for a clear compile error if the
    // type doesn't implement `Error`. Going through `make_where_clause` handles
    // missing, empty (`where;`), and non-comma-terminated where clauses alike.
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let mut generics = input.generics.clone();
    generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#type_name #ty_generics: ::core::error::Error));
    let (impl_generics, _, where_clause) = generics.split_for_impl();

    // The `From` impl lands in the consumer crate, which may be `#![no_std]`
    // where `ToString` isn't in the prelude. Pull in `alloc` via a block-local
    // `extern crate` (valid under both `std` and `no_std`) and call
    // `ToString::to_string` fully qualified, so the consumer needs no imports.
    Ok(quote! {
        impl #impl_generics ::core::convert::From<#type_name #ty_generics> for ::wasm_bindgen::JsValue
        #where_clause
        {
            fn from(err: #type_name #ty_generics) -> Self {
                extern crate alloc as _wasm_bindgen_error_alloc;

                let js_err = ::js_sys::Error::new(
                    &_wasm_bindgen_error_alloc::string::ToString::to_string(&err),
                );
                js_err.set_name(#js_name_str);
                js_err.into()
            }
        }
    })
}

/// Extract `js_name = "..."` from the type's `#[wasm_error(...)]` attributes.
///
/// Every `#[wasm_error]` attribute is parsed (so typos are always reported),
/// and a second `js_name`, in the same or another attribute, is an error.
fn extract_js_name(attrs: &[Attribute]) -> syn::Result<Option<String>> {
    let mut js_name: Option<LitStr> = None;

    for attr in attrs.iter().filter(|attr| is_wasm_error(attr)) {
        attr.parse_nested_meta(|meta| {
            if !meta.path.is_ident("js_name") {
                return Err(meta.error("unknown attribute, expected `js_name = \"...\"`"));
            }

            let lit: LitStr = meta.value()?.parse()?;
            if js_name.is_some() {
                return Err(syn::Error::new(
                    lit.span(),
                    "duplicate `js_name`; specify it once",
                ));
            }

            js_name = Some(lit);
            Ok(())
        })?;
    }

    Ok(js_name.map(|lit| lit.value()))
}

/// `wasm_error` is registered as a derive helper, so rustc accepts it on
/// variants and fields too. It has no meaning there; reject it rather than
/// silently ignoring it.
fn reject_misplaced_attrs(data: &Data) -> syn::Result<()> {
    let misplaced = match data {
        Data::Struct(data) => field_attrs(&data.fields).find(|attr| is_wasm_error(attr)),
        Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|variant| variant.attrs.iter().chain(field_attrs(&variant.fields)))
            .find(|attr| is_wasm_error(attr)),
        Data::Union(data) => data
            .fields
            .named
            .iter()
            .flat_map(|field| &field.attrs)
            .find(|attr| is_wasm_error(attr)),
    };

    misplaced.map_or(Ok(()), |attr| {
        Err(syn::Error::new_spanned(
            attr,
            "`#[wasm_error]` is only supported on the type itself, not on variants or fields",
        ))
    })
}

fn field_attrs(fields: &Fields) -> impl Iterator<Item = &Attribute> {
    fields.iter().flat_map(|field| &field.attrs)
}

fn is_wasm_error(attr: &Attribute) -> bool {
    attr.path().is_ident("wasm_error")
}

/// Strip a leading `Wasm` word from a CamelCase type name.
///
/// `WasmFooError` → `"FooError"`, `FooError` → `"FooError"`,
/// `WasmtimeError` → `"WasmtimeError"`.
fn strip_wasm_prefix(name: &str) -> &str {
    match name.strip_prefix("Wasm") {
        Some(rest) if rest.starts_with(char::is_uppercase) => rest,
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: run the macro and parse its output as an `impl` item, so every
    /// test also checks that the expansion is syntactically valid Rust.
    fn expand_impl(input: proc_macro2::TokenStream) -> syn::ItemImpl {
        let derive_input: DeriveInput = syn::parse2(input).expect("failed to parse derive input");
        let tokens = wasm_error_impl(&derive_input).expect("macro expansion failed");
        let parsed = syn::parse2(tokens.clone());
        assert!(parsed.is_ok(), "expansion is not a valid impl:\n{tokens}");
        parsed.expect("checked above")
    }

    /// Helper: run the macro and return the (validated) output as a `String`.
    fn expand(input: proc_macro2::TokenStream) -> String {
        let item = expand_impl(input);
        quote!(#item).to_string()
    }

    fn where_predicates(item: &syn::ItemImpl) -> Vec<String> {
        item.generics
            .where_clause
            .iter()
            .flat_map(|wc| &wc.predicates)
            .map(|pred| quote!(#pred).to_string())
            .collect()
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
    fn strip_wasm_prefix_exact_wasm_unchanged() {
        assert_eq!(strip_wasm_prefix("Wasm"), "Wasm");
    }

    #[test]
    fn strip_wasm_prefix_requires_word_boundary() {
        assert_eq!(strip_wasm_prefix("WasmtimeError"), "WasmtimeError");
        assert_eq!(strip_wasm_prefix("WasmerError"), "WasmerError");
        assert_eq!(strip_wasm_prefix("WasmiTrap"), "WasmiTrap");
        assert_eq!(strip_wasm_prefix("Wasm_Error"), "Wasm_Error");
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
    fn error_bound_appended_to_where_clause_without_trailing_comma() {
        let item = expand_impl(quote! {
            #[derive(Debug)]
            pub struct WasmBoundedError<T>(T) where T: Clone;
        });

        assert_eq!(
            where_predicates(&item),
            [
                "T : Clone",
                "WasmBoundedError < T > : :: core :: error :: Error"
            ],
        );
    }

    #[test]
    fn error_bound_appended_to_where_clause_with_trailing_comma() {
        let item = expand_impl(quote! {
            #[derive(Debug)]
            pub struct WasmBoundedError<T>(T) where T: Clone,;
        });

        assert_eq!(
            where_predicates(&item),
            [
                "T : Clone",
                "WasmBoundedError < T > : :: core :: error :: Error"
            ],
        );
    }

    #[test]
    fn error_bound_added_to_empty_where_clause() {
        let item = expand_impl(quote! {
            #[derive(Debug)]
            pub struct WasmBoundedError<T>(T) where;
        });

        assert_eq!(
            where_predicates(&item),
            ["WasmBoundedError < T > : :: core :: error :: Error"],
        );
    }

    #[test]
    fn error_bound_appended_to_named_struct_where_clause() {
        let item = expand_impl(quote! {
            #[derive(Debug)]
            pub struct WasmBoundedError<T> where T: Clone + Send { inner: T }
        });

        assert_eq!(
            where_predicates(&item),
            [
                "T : Clone + Send",
                "WasmBoundedError < T > : :: core :: error :: Error"
            ],
        );
    }

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

    #[test]
    fn type_named_wasm_keeps_its_name() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct Wasm;
        });

        assert!(
            output.contains(r#"set_name ("Wasm")"#),
            "a bare `Wasm` has no following word to strip down to.\nOutput: {output}",
        );
    }

    #[test]
    fn wasm_prefix_without_word_boundary_is_kept() {
        let output = expand(quote! {
            #[derive(Debug)]
            pub struct WasmtimeError;
        });

        assert!(
            output.contains(r#"set_name ("WasmtimeError")"#),
            "`Wasmtime` is one word; the prefix must not be stripped.\nOutput: {output}",
        );
    }

    #[test]
    fn error_on_empty_explicit_js_name() {
        let err = expand_err(quote! {
            #[derive(Debug)]
            #[wasm_error(js_name = "")]
            pub struct WasmFooError;
        });

        assert!(
            err.contains("resolved JS error name is empty"),
            "must reject empty explicit js_name.\nError: {err}",
        );
    }

    #[test]
    fn error_on_whitespace_only_js_name() {
        let err = expand_err(quote! {
            #[derive(Debug)]
            #[wasm_error(js_name = "   ")]
            pub struct WasmFooError;
        });

        assert!(
            err.contains("resolved JS error name is empty"),
            "must reject whitespace-only js_name.\nError: {err}",
        );
    }

    #[test]
    fn error_on_duplicate_js_name_across_attrs() {
        let err = expand_err(quote! {
            #[derive(Debug)]
            #[wasm_error(js_name = "First")]
            #[wasm_error(js_name = "Second")]
            pub struct WasmDuplicateError;
        });

        assert!(err.contains("duplicate `js_name`"), "Error: {err}");
    }

    #[test]
    fn error_on_duplicate_js_name_in_one_attr() {
        let err = expand_err(quote! {
            #[derive(Debug)]
            #[wasm_error(js_name = "First", js_name = "Second")]
            pub struct WasmDuplicateError;
        });

        assert!(err.contains("duplicate `js_name`"), "Error: {err}");
    }

    #[test]
    fn error_on_typo_in_later_attr() {
        let err = expand_err(quote! {
            #[derive(Debug)]
            #[wasm_error(js_name = "First")]
            #[wasm_error(jsname = "Second")]
            pub struct WasmTypoError;
        });

        assert!(err.contains("unknown attribute"), "Error: {err}");
    }

    #[test]
    fn error_on_attr_on_enum_variant() {
        let err = expand_err(quote! {
            #[derive(Debug)]
            pub enum WasmVariantError {
                #[wasm_error(js_name = "Timeout")]
                Timeout,
            }
        });

        assert!(
            err.contains("only supported on the type itself"),
            "Error: {err}"
        );
    }

    #[test]
    fn error_on_attr_on_struct_field() {
        let err = expand_err(quote! {
            #[derive(Debug)]
            pub struct WasmFieldError(#[wasm_error(js_name = "Inner")] u32);
        });

        assert!(
            err.contains("only supported on the type itself"),
            "Error: {err}"
        );
    }

    #[test]
    fn error_on_attr_on_variant_field() {
        let err = expand_err(quote! {
            #[derive(Debug)]
            pub enum WasmVariantFieldError {
                Io { #[wasm_error(js_name = "Inner")] code: u32 },
            }
        });

        assert!(
            err.contains("only supported on the type itself"),
            "Error: {err}"
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn strip_wasm_prefix_properties() {
        bolero::check!().with_type::<String>().for_each(|name| {
            let result = strip_wasm_prefix(name);

            match name.strip_prefix("Wasm") {
                // Stripped exactly when `Wasm` is followed by an uppercase char
                Some(rest) if rest.starts_with(char::is_uppercase) => assert_eq!(result, rest),
                _ => assert_eq!(result, name),
            }

            // Stripping never yields an empty name from a non-empty one
            assert!(
                name.is_empty() || !result.is_empty(),
                "empty result for {name:?}"
            );
        });
    }
}
