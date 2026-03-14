#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(not(doctest), doc = include_str!("../README.md"))]

extern crate alloc;

use alloc::{
    format,
    string::{String, ToString},
};
use heck::ToSnakeCase;
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    spanned::Spanned,
    token::Comma,
    Attribute, ImplItem, ImplItemFn, ItemImpl, Meta, Result, Token,
};

/// Generates boilerplate to upcast from a duck-typed JS reference to a concrete
/// Rust type implementing that interface.
///
/// This is a light hack that provides a clean, `JsCast`-compatible way to use
/// Rust-exported structs with `wasm-bindgen`. The main caveat is that it assumes
/// that cloning is relatively cheap on the struct in question.
///
/// For more detail, see the module documentation.
#[proc_macro_attribute]
pub fn wasm_refgen(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as Args);
    let impl_block = parse_macro_input!(item as ItemImpl);
    wasm_refgen_impl(args, impl_block).into()
}

fn wasm_refgen_impl(args: Args, mut impl_block: ItemImpl) -> proc_macro2::TokenStream {
    let js_ref_ident = args.js_ref;

    if let Some((_, path, _)) = &impl_block.trait_ {
        return syn::Error::new(
            path.span(),
            "#[wasm_refgen] must be used on an inherent impl, not a trait impl",
        )
        .to_compile_error();
    }

    // Get the type name (e.g., JsFoo)
    let ty_ident = match &*impl_block.self_ty {
        syn::Type::Path(tp) => tp.path.segments.last().unwrap().ident.clone(),
        _ => {
            return syn::Error::new_spanned(&impl_block.self_ty, "expected a simple type name")
                .to_compile_error();
        }
    };

    let core_name = ty_ident.to_string();
    let core_snake = core_name.to_snake_case();

    let js_class_ident: Ident = if let Some(js_class) = find_js_class(&impl_block.attrs) {
        match to_ident_or_err(&js_class, ty_ident.span()) {
            Ok(id) => id,
            Err(e) => return e.to_compile_error(),
        }
    } else {
        return syn::Error::new(
            ty_ident.span(),
            "wasm_refgen: missing js_ref argument and no `js_class = ...` found on #[wasm_bindgen]",
        )
        .to_compile_error();
    };

    let upcast_tag = format!("__wasm_refgen_to{}", core_name);
    let method_ident = format_ident!("__wasm_refgen_to_{}", core_snake);

    let injected_doc = format!("Upcasts; to the JS-import type for [`{ty_ident}`].");
    let js_ty_doc = format!(
        "The JS-import type for [`{ty_ident}`].\n\nThis lets you use the duck typed interface to convert from JS values."
    );
    let method_doc = format!("Use the JS duck type interface to upcast to [`{ty_ident}`].");

    let already_present = impl_block.items.iter().any(|it| {
        if let ImplItem::Fn(ImplItemFn { sig, .. }) = it {
            sig.ident == method_ident
        } else {
            false
        }
    });

    if !already_present {
        let injected: ImplItem = syn::parse_quote! {
            #[doc = #injected_doc]
            #[::wasm_bindgen::prelude::wasm_bindgen(js_name = #upcast_tag)]
            pub fn #method_ident(&self) -> Self {
                self.clone()
            }
        };
        impl_block.items.push(injected);
    }

    let extras = quote! {
        impl ::from_js_ref::FromJsRef for #ty_ident {
            type JsRef = #js_ref_ident;

            #[inline]
            fn from_js_ref(castable: &Self::JsRef) -> Self {
                castable.#method_ident()
            }
        }

        impl From<#ty_ident> for #js_ref_ident {
            fn from(v: #ty_ident) -> Self {
                ::wasm_bindgen::JsValue::from(v).unchecked_into()
            }
        }

        impl From<&#js_ref_ident> for #ty_ident {
            fn from(js_ref: &#js_ref_ident) -> Self {
                js_ref.#method_ident()
            }
        }

        #[::wasm_bindgen::prelude::wasm_bindgen]
        extern "C" {
            #[doc = #js_ty_doc]
            #[::wasm_bindgen::prelude::wasm_bindgen(js_name = #js_class_ident, typescript_type = #js_class_ident)]
            pub type #js_ref_ident;

            #[doc = #method_doc]
            #[::wasm_bindgen::prelude::wasm_bindgen(method, js_name = #upcast_tag)]
            pub fn #method_ident(this: &#js_ref_ident) -> #ty_ident;
        }
    };

    quote!(#impl_block #extras)
}

struct Args {
    js_ref: syn::Ident,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut js_ref: Option<syn::Ident> = None;

        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            if key == "js_ref" {
                js_ref = Some(input.parse()?);
            } else {
                return Err(syn::Error::new(
                    key.span(),
                    "unknown arg; expected `js_ref` or `ts`",
                ));
            }

            if input.peek(Comma) {
                let _ = input.parse::<Comma>();
            }
        }

        let js_ref = js_ref.ok_or_else(|| {
            syn::Error::new(input.span(), "missing required arg: js_ref = <Ident>")
        })?;

        Ok(Self { js_ref })
    }
}

fn wasm_bindgen_args(attr: &Attribute) -> Option<Punctuated<Meta, Token![,]>> {
    if !attr.path().is_ident("wasm_bindgen") {
        return None;
    }
    attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .ok()
}

fn meta_value_as_string(meta: &Meta) -> Option<String> {
    use syn::{Expr, ExprLit, ExprPath};
    let Meta::NameValue(nv) = meta else {
        return None;
    };

    // Try string literal first: js_class = "Foo"
    if let Expr::Lit(ExprLit {
        lit: syn::Lit::Str(s),
        ..
    }) = &nv.value
    {
        return Some(s.value());
    }

    // Then bare ident: js_class = Foo
    if let Expr::Path(ExprPath { path, .. }) = &nv.value {
        if let Some(seg) = path.segments.last() {
            return Some(seg.ident.to_string());
        }
    }

    None
}

fn find_js_class(attrs: &[Attribute]) -> Option<String> {
    for a in attrs {
        let Some(metas) = wasm_bindgen_args(a) else {
            continue;
        };
        for m in metas {
            if let Some(val) = match &m {
                Meta::NameValue(nv) if nv.path.is_ident("js_class") => meta_value_as_string(&m),
                _ => None,
            } {
                return Some(val);
            }
        }
    }
    None
}

fn to_ident_or_err(s: &str, span: Span) -> Result<Ident> {
    if syn::parse_str::<Ident>(s).is_ok() {
        Ok(Ident::new(s, span))
    } else {
        Err(syn::Error::new(
            span,
            format!("`{s}` is not a valid Rust identifier"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: run the macro transform on the given attribute and impl block
    /// token streams, returning the output as a `String`.
    fn expand(attr: proc_macro2::TokenStream, item: proc_macro2::TokenStream) -> String {
        let args: Args = syn::parse2(attr).expect("failed to parse macro args");
        let impl_block: ItemImpl = syn::parse2(item).expect("failed to parse impl block");
        wasm_refgen_impl(args, impl_block).to_string()
    }

    #[test]
    fn extern_type_has_js_name() {
        let output = expand(
            quote!(js_ref = JsFoo),
            quote! {
                #[wasm_bindgen(js_class = "Foo")]
                impl WasmFoo {}
            },
        );

        assert!(
            output.contains("js_name = Foo"),
            "extern type declaration must include `js_name = Foo` \
             so instanceof targets the correct JS class.\n\
             Output: {output}",
        );
    }

    #[test]
    fn extern_type_has_typescript_type() {
        let output = expand(
            quote!(js_ref = JsFoo),
            quote! {
                #[wasm_bindgen(js_class = "Foo")]
                impl WasmFoo {}
            },
        );

        assert!(
            output.contains("typescript_type = Foo"),
            "extern type declaration must include `typescript_type`.\n\
             Output: {output}",
        );
    }

    #[test]
    fn extern_type_js_name_matches_js_class() {
        let output = expand(
            quote!(js_ref = JsCommitWithBlob),
            quote! {
                #[wasm_bindgen(js_class = "CommitWithBlob")]
                impl WasmCommitWithBlob {}
            },
        );

        assert!(
            output.contains("js_name = CommitWithBlob"),
            "extern type `js_name` must match the `js_class` value, not the Rust ident.\n\
             Output: {output}",
        );
    }

    #[test]
    fn generates_from_js_ref_impl() {
        let output = expand(
            quote!(js_ref = JsFoo),
            quote! {
                #[wasm_bindgen(js_class = "Foo")]
                impl WasmFoo {}
            },
        );

        assert!(
            output.contains("FromJsRef"),
            "must generate a FromJsRef impl.\nOutput: {output}",
        );
        assert!(
            output.contains("type JsRef = JsFoo"),
            "FromJsRef::JsRef must be the js_ref ident.\nOutput: {output}",
        );
    }

    #[test]
    fn generates_upcast_method() {
        let output = expand(
            quote!(js_ref = JsFoo),
            quote! {
                #[wasm_bindgen(js_class = "Foo")]
                impl WasmFoo {}
            },
        );

        assert!(
            output.contains("__wasm_refgen_to_wasm_foo"),
            "must inject the upcast clone method.\nOutput: {output}",
        );
        assert!(
            output.contains("__wasm_refgen_toWasmFoo"),
            "must generate the JS-side method tag.\nOutput: {output}",
        );
    }

    #[test]
    fn generates_from_impls() {
        let output = expand(
            quote!(js_ref = JsFoo),
            quote! {
                #[wasm_bindgen(js_class = "Foo")]
                impl WasmFoo {}
            },
        );

        assert!(
            output.contains("From < WasmFoo > for JsFoo")
                || output.contains("From<WasmFoo> for JsFoo"),
            "must generate From<WasmFoo> for JsFoo.\nOutput: {output}",
        );
        assert!(
            output.contains("From < & JsFoo > for WasmFoo")
                || output.contains("From<&JsFoo> for WasmFoo"),
            "must generate From<&JsFoo> for WasmFoo.\nOutput: {output}",
        );
    }

    #[test]
    fn error_on_trait_impl() {
        let output = expand(
            quote!(js_ref = JsFoo),
            quote! {
                #[wasm_bindgen(js_class = "Foo")]
                impl SomeTrait for WasmFoo {}
            },
        );

        assert!(
            output.contains("compile_error"),
            "trait impl must produce a compile error.\nOutput: {output}",
        );
    }

    #[test]
    fn error_on_missing_js_class() {
        let output = expand(
            quote!(js_ref = JsFoo),
            quote! {
                impl WasmFoo {}
            },
        );

        assert!(
            output.contains("compile_error"),
            "missing js_class must produce a compile error.\nOutput: {output}",
        );
    }
}
