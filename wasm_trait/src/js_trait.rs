//! Implementation of the `#[js_trait(js_type = JsTransport)]` attribute macro.
//!
//! Placed on a Rust trait definition, generates:
//! 1. `typescript_custom_section` with a precise TS interface
//! 2. `extern "C"` block with `#[wasm_bindgen]` bindings
//! 3. Rust trait (wasm_bindgen attrs stripped, `async` → RPITIT)
//! 4. `impl Trait for ExternType` (sync delegation + async `JsFuture` + `unchecked_into`)

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    FnArg, ItemTrait, ReturnType, Token, TraitItem, TraitItemFn,
};

use crate::shared::{
    attrs::strip_wasm_bindgen,
    method::{arg_names, has_self_receiver, to_rpitit_signature},
    naming::extern_fn_ident,
    ts_types::{async_return_to_ts, sync_return_to_ts},
};

/// Parsed arguments to `#[js_trait(js_type = JsTransport, js_name = Transport)]`.
pub struct JsTraitArgs {
    /// Required: Rust ident for the generated extern type.
    pub js_type: Ident,
    /// Optional: JS/TS interface name. Defaults to the trait name.
    pub js_name: Option<String>,
}

impl Parse for JsTraitArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut js_type: Option<Ident> = None;
        let mut js_name: Option<String> = None;

        let pairs = Punctuated::<syn::MetaNameValue, Token![,]>::parse_terminated(input)?;
        for pair in pairs {
            let key = pair
                .path
                .get_ident()
                .ok_or_else(|| syn::Error::new(pair.path.span(), "expected identifier"))?
                .to_string();

            match key.as_str() {
                "js_type" => {
                    if let syn::Expr::Path(expr_path) = &pair.value {
                        if let Some(ident) = expr_path.path.get_ident() {
                            js_type = Some(ident.clone());
                        } else {
                            return Err(syn::Error::new(
                                pair.value.span(),
                                "js_type must be a simple identifier",
                            ));
                        }
                    } else {
                        return Err(syn::Error::new(
                            pair.value.span(),
                            "js_type must be a simple identifier",
                        ));
                    }
                }
                "js_name" => {
                    if let syn::Expr::Path(expr_path) = &pair.value {
                        if let Some(ident) = expr_path.path.get_ident() {
                            js_name = Some(ident.to_string());
                        } else {
                            return Err(syn::Error::new(
                                pair.value.span(),
                                "js_name must be a simple identifier",
                            ));
                        }
                    } else if let syn::Expr::Lit(lit) = &pair.value {
                        if let syn::Lit::Str(s) = &lit.lit {
                            js_name = Some(s.value());
                        } else {
                            return Err(syn::Error::new(
                                pair.value.span(),
                                "js_name must be an identifier or string literal",
                            ));
                        }
                    } else {
                        return Err(syn::Error::new(
                            pair.value.span(),
                            "js_name must be an identifier or string literal",
                        ));
                    }
                }
                other => {
                    return Err(syn::Error::new(
                        pair.path.span(),
                        format!("unknown attribute `{other}`, expected `js_type` or `js_name`"),
                    ));
                }
            }
        }

        let js_type = js_type.ok_or_else(|| {
            syn::Error::new(input.span(), "js_trait requires `js_type = Identifier`")
        })?;

        Ok(Self { js_type, js_name })
    }
}

/// Parsed info about a single trait method.
struct MethodInfo<'a> {
    method: &'a TraitItemFn,
    is_async: bool,
    has_self: bool,
    js_name: Option<String>,
}

/// Extract the `js_name` value from wasm_bindgen attrs on a trait method.
fn extract_js_name(method: &TraitItemFn) -> Option<String> {
    for attr in &method.attrs {
        if !attr.path().is_ident("wasm_bindgen") {
            continue;
        }
        if let Ok(nested) =
            attr.parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)
        {
            for meta in &nested {
                if let syn::Meta::NameValue(nv) = meta {
                    if nv.path.is_ident("js_name") {
                        if let syn::Expr::Lit(lit) = &nv.value {
                            if let syn::Lit::Str(s) = &lit.lit {
                                return Some(s.value());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Collect wasm_bindgen meta items from a method's attrs.
fn collect_wb_meta(method: &TraitItemFn) -> Vec<syn::Meta> {
    let mut result = Vec::new();
    for attr in &method.attrs {
        if !attr.path().is_ident("wasm_bindgen") {
            continue;
        }
        if let Ok(nested) =
            attr.parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)
        {
            for meta in nested {
                result.push(meta);
            }
        }
    }
    result
}

/// Core implementation of `#[js_trait]`.
pub fn js_trait_impl(args: JsTraitArgs, trait_def: ItemTrait) -> TokenStream {
    let trait_name = &trait_def.ident;
    let trait_vis = &trait_def.vis;
    let js_type_ident = &args.js_type;
    let ts_interface_name = args.js_name.unwrap_or_else(|| trait_name.to_string());

    // Validate: no generics
    if !trait_def.generics.params.is_empty() {
        return syn::Error::new(
            trait_def.generics.span(),
            "js_trait does not support generic traits",
        )
        .to_compile_error();
    }

    // Validate: no associated types or constants
    for item in &trait_def.items {
        match item {
            TraitItem::Type(t) => {
                return syn::Error::new(t.span(), "js_trait does not support associated types")
                    .to_compile_error();
            }
            TraitItem::Const(c) => {
                return syn::Error::new(c.span(), "js_trait does not support associated constants")
                    .to_compile_error();
            }
            _ => {}
        }
    }

    // Parse all methods
    let methods: Vec<MethodInfo> = trait_def
        .items
        .iter()
        .filter_map(|item| {
            if let TraitItem::Fn(method) = item {
                let is_async = method.sig.asyncness.is_some();
                let has_self = has_self_receiver(&method.sig);
                let js_name = extract_js_name(method);
                Some(MethodInfo {
                    method,
                    is_async,
                    has_self,
                    js_name,
                })
            } else {
                None
            }
        })
        .collect();

    // Validate: async methods must return Result
    for mi in &methods {
        if mi.is_async {
            let returns_result = match &mi.method.sig.output {
                ReturnType::Type(_, ty) => {
                    if let syn::Type::Path(tp) = ty.as_ref() {
                        tp.path
                            .segments
                            .last()
                            .map_or(false, |seg| seg.ident == "Result")
                    } else {
                        false
                    }
                }
                ReturnType::Default => false,
            };

            if !returns_result {
                return syn::Error::new(
                    mi.method.sig.span(),
                    "js_trait async methods must return Result (JS promises can reject)",
                )
                .to_compile_error();
            }
        }
    }

    // Validate: only &self or no receiver
    for mi in &methods {
        for arg in &mi.method.sig.inputs {
            if let FnArg::Receiver(recv) = arg {
                if recv.reference.is_none() || recv.mutability.is_some() {
                    return syn::Error::new(
                        recv.span(),
                        "js_trait methods must use `&self` or no receiver",
                    )
                    .to_compile_error();
                }
            }
        }
    }

    // ── 1. TypeScript custom section ──────────────────────────────

    let ts_methods: Vec<String> = methods
        .iter()
        .map(|mi| {
            let default_name = mi.method.sig.ident.to_string();
            let method_name = mi.js_name.as_deref().unwrap_or(&default_name);

            // Build TS parameter list
            let ts_params: Vec<String> = mi
                .method
                .sig
                .inputs
                .iter()
                .filter_map(|arg| {
                    if let FnArg::Typed(pat_type) = arg {
                        let param_name = match pat_type.pat.as_ref() {
                            syn::Pat::Ident(pi) => pi.ident.to_string(),
                            _ => "_".into(),
                        };
                        let ts_type = crate::shared::ts_types::rust_type_to_ts(&pat_type.ty);
                        Some(format!("{param_name}: {ts_type}"))
                    } else {
                        None
                    }
                })
                .collect();

            let params_str = ts_params.join(", ");

            // Build TS return type
            let ts_return = if mi.is_async {
                async_return_to_ts(match &mi.method.sig.output {
                    ReturnType::Type(_, ty) => ty,
                    ReturnType::Default => unreachable!("validated above"),
                })
            } else {
                sync_return_to_ts(&mi.method.sig.output)
            };

            format!("    {method_name}({params_str}): {ts_return};")
        })
        .collect();

    let ts_body = ts_methods.join("\n");
    let ts_section_str = format!("export interface {ts_interface_name} {{\n{ts_body}\n}}");

    let ts_const_name = Ident::new(
        &format!("__WASM_TRAIT_TS_{}", trait_name.to_string().to_uppercase()),
        trait_name.span(),
    );

    let ts_section = quote! {
        #[wasm_bindgen(typescript_custom_section)]
        const #ts_const_name: &str = #ts_section_str;
    };

    // ── 2. extern "C" block ──────────────────────────────────────

    let extern_fns: Vec<TokenStream> = methods
        .iter()
        .map(|mi| {
            let method_name = &mi.method.sig.ident;
            let extern_name = extern_fn_ident(method_name);

            // Collect wasm_bindgen meta items
            let mut wb_metas: Vec<TokenStream> = Vec::new();

            // Add `method` if has &self
            if mi.has_self {
                wb_metas.push(quote!(method));
            }

            // Forward existing wasm_bindgen attrs
            for meta in collect_wb_meta(mi.method) {
                wb_metas.push(quote!(#meta));
            }

            // Build parameter list: this: &JsType for &self methods
            let params: Vec<TokenStream> = mi
                .method
                .sig
                .inputs
                .iter()
                .map(|arg| match arg {
                    FnArg::Receiver(_) => {
                        quote!(this: &#js_type_ident)
                    }
                    FnArg::Typed(pat_type) => {
                        let pat = &pat_type.pat;
                        let ty = &pat_type.ty;
                        quote!(#pat: #ty)
                    }
                })
                .collect();

            // Return type: async → Promise, otherwise passthrough
            let ret = if mi.is_async {
                quote!(-> ::js_sys::Promise)
            } else {
                match &mi.method.sig.output {
                    ReturnType::Default => quote!(),
                    ReturnType::Type(arrow, ty) => quote!(#arrow #ty),
                }
            };

            quote! {
                #[wasm_bindgen(#(#wb_metas),*)]
                fn #extern_name(#(#params),*) #ret;
            }
        })
        .collect();

    let extern_block = quote! {
        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(typescript_type = #ts_interface_name)]
            #trait_vis type #js_type_ident;

            #(#extern_fns)*
        }
    };

    // ── 3. Rust trait (cleaned up) ───────────────────────────────

    let trait_methods: Vec<TokenStream> = methods
        .iter()
        .map(|mi| {
            let clean_attrs = strip_wasm_bindgen(&mi.method.attrs);
            let rpitit_sig = to_rpitit_signature(&mi.method.sig);

            quote! {
                #(#clean_attrs)*
                #rpitit_sig;
            }
        })
        .collect();

    let trait_attrs: Vec<_> = trait_def.attrs.iter().collect();
    let trait_supertraits = &trait_def.supertraits;
    let colon_token = trait_def.colon_token;

    let trait_output = if trait_supertraits.is_empty() {
        quote! {
            #(#trait_attrs)*
            #trait_vis trait #trait_name {
                #(#trait_methods)*
            }
        }
    } else {
        quote! {
            #(#trait_attrs)*
            #trait_vis trait #trait_name #colon_token #trait_supertraits {
                #(#trait_methods)*
            }
        }
    };

    // ── 4. impl Trait for ExternType ─────────────────────────────

    let impl_methods: Vec<TokenStream> = methods
        .iter()
        .map(|mi| {
            let method_name = &mi.method.sig.ident;
            let extern_name = extern_fn_ident(method_name);
            let rpitit_sig = to_rpitit_signature(&mi.method.sig);
            let arg_names: Vec<_> = arg_names(&mi.method.sig);

            if mi.is_async {
                // Async: wrap with JsFuture + unchecked_into
                let extern_call = if mi.has_self {
                    quote! { #extern_name(self, #(#arg_names),*) }
                } else {
                    quote! { #extern_name(#(#arg_names),*) }
                };

                // Extract Ok and Err types from Result<T, E>
                let (ok_ty, _err_ty) = extract_result_types(&mi.method.sig.output);

                let ok_conversion = if is_unit_type(&ok_ty) {
                    quote! { ::core::result::Result::Ok(()) }
                } else {
                    quote! {
                        ::core::result::Result::Ok(
                            ::wasm_bindgen::JsCast::unchecked_into(__v)
                        )
                    }
                };

                quote! {
                    #rpitit_sig {
                        async move {
                            let __promise = #extern_call;
                            match ::wasm_bindgen_futures::JsFuture::from(__promise).await {
                                ::core::result::Result::Ok(__v) => #ok_conversion,
                                ::core::result::Result::Err(__e) => {
                                    ::core::result::Result::Err(
                                        ::wasm_bindgen::JsCast::unchecked_into(__e)
                                    )
                                }
                            }
                        }
                    }
                }
            } else {
                // Sync: direct delegation
                let call = if mi.has_self {
                    quote! { #extern_name(self, #(#arg_names),*) }
                } else {
                    quote! { #extern_name(#(#arg_names),*) }
                };

                quote! {
                    #rpitit_sig {
                        #call
                    }
                }
            }
        })
        .collect();

    let impl_block = quote! {
        impl #trait_name for #js_type_ident {
            #(#impl_methods)*
        }
    };

    // ── Combine everything ───────────────────────────────────────

    quote! {
        #ts_section
        #extern_block
        #trait_output
        #impl_block
    }
}

/// Extract `(Ok_type, Err_type)` from a `ReturnType` that's `Result<T, E>`.
fn extract_result_types(ret: &ReturnType) -> (TokenStream, TokenStream) {
    if let ReturnType::Type(_, ty) = ret {
        if let syn::Type::Path(tp) = ty.as_ref() {
            if let Some(seg) = tp.path.segments.last() {
                if seg.ident == "Result" {
                    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                        let mut iter = args.args.iter();
                        let ok = iter.next().map(|a| quote!(#a)).unwrap_or(quote!(()));
                        let err = iter
                            .next()
                            .map(|a| quote!(#a))
                            .unwrap_or(quote!(::wasm_bindgen::JsValue));
                        return (ok, err);
                    }
                }
            }
        }
    }
    (quote!(()), quote!(::wasm_bindgen::JsValue))
}

/// Check if a token stream represents the unit type `()`.
fn is_unit_type(ty: &TokenStream) -> bool {
    let s = ty.to_string();
    s == "()" || s.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use quote::quote;

    fn expand(attr: proc_macro2::TokenStream, item: proc_macro2::TokenStream) -> String {
        let args: JsTraitArgs = syn::parse2(attr).expect("failed to parse js_trait args");
        let trait_def: ItemTrait = syn::parse2(item).expect("failed to parse trait");
        js_trait_impl(args, trait_def).to_string()
    }

    #[test]
    fn generates_extern_type() {
        let output = expand(
            quote!(js_type = JsTransport),
            quote! {
                pub trait Transport {
                    fn js_name(&self) -> String;
                }
            },
        );

        assert!(
            output.contains("type JsTransport"),
            "must generate extern type JsTransport.\nOutput: {output}",
        );
        assert!(
            output.contains("typescript_type = \"Transport\""),
            "must set typescript_type to trait name.\nOutput: {output}",
        );
    }

    #[test]
    fn generates_typescript_interface() {
        let output = expand(
            quote!(js_type = JsTransport),
            quote! {
                pub trait Transport {
                    #[wasm_bindgen(js_name = "getName")]
                    fn js_name(&self) -> String;
                }
            },
        );

        assert!(
            output.contains("export interface Transport"),
            "must generate TS interface named Transport.\nOutput: {output}",
        );
        assert!(
            output.contains("getName"),
            "must use js_name in TS interface.\nOutput: {output}",
        );
    }

    #[test]
    fn js_name_override() {
        let output = expand(
            quote!(js_type = JsTransport, js_name = MyTransport),
            quote! {
                pub trait Transport {
                    fn js_name(&self) -> String;
                }
            },
        );

        assert!(
            output.contains("export interface MyTransport"),
            "must use js_name override for TS interface.\nOutput: {output}",
        );
        assert!(
            output.contains("typescript_type = \"MyTransport\""),
            "must use js_name override for typescript_type.\nOutput: {output}",
        );
    }

    #[test]
    fn generates_trait_without_wasm_bindgen_attrs() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "put")]
                    fn js_put(&self, value: u32) -> u32;
                }
            },
        );

        // Find the trait definition (not in extern block, not in impl block)
        assert!(
            output.contains("pub trait Foo"),
            "must emit the Rust trait.\nOutput: {output}",
        );

        // The trait shouldn't have wasm_bindgen attrs on methods
        // Find the trait section
        let trait_start = output.find("pub trait Foo").expect("must have trait");
        // Find next `impl` or `const` after the trait (end of trait block)
        let trait_end = output[trait_start..]
            .find("impl Foo for JsFoo")
            .map(|i| i + trait_start)
            .unwrap_or(output.len());
        let trait_section = &output[trait_start..trait_end];

        assert!(
            !trait_section.contains("wasm_bindgen"),
            "trait must not contain wasm_bindgen attrs.\nTrait section: {trait_section}",
        );
    }

    #[test]
    fn generates_impl_for_extern_type() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    fn js_put(&self, value: u32) -> u32;
                }
            },
        );

        assert!(
            output.contains("impl Foo for JsFoo"),
            "must generate impl Trait for ExternType.\nOutput: {output}",
        );
    }

    #[test]
    fn extern_fns_prefixed() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    fn js_put(&self, value: u32) -> u32;
                }
            },
        );

        assert!(
            output.contains("__wasm_trait_js_put"),
            "extern fns must be prefixed with __wasm_trait_.\nOutput: {output}",
        );
    }

    #[test]
    fn async_method_returns_promise_in_extern() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    async fn js_save(&self, value: u32) -> Result<(), JsValue>;
                }
            },
        );

        // Extern block should have Promise return
        let extern_start = output.find("extern \"C\"").expect("must have extern block");
        let extern_end = output[extern_start..]
            .find("pub trait")
            .map(|i| i + extern_start)
            .unwrap_or(output.len());
        let extern_section = &output[extern_start..extern_end];

        assert!(
            extern_section.contains("Promise"),
            "async extern fns must return Promise.\nExtern: {extern_section}",
        );
    }

    #[test]
    fn async_method_uses_rpitit_in_trait() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    async fn js_save(&self, value: u32) -> Result<(), JsValue>;
                }
            },
        );

        let trait_start = output.find("pub trait Foo").expect("must have trait");
        let trait_end = output[trait_start..]
            .find("impl Foo for JsFoo")
            .map(|i| i + trait_start)
            .unwrap_or(output.len());
        let trait_section = &output[trait_start..trait_end];

        assert!(
            trait_section.contains("Future"),
            "async trait methods must use RPITIT.\nTrait: {trait_section}",
        );
        assert!(
            !trait_section.contains("async"),
            "trait must not have async keyword.\nTrait: {trait_section}",
        );
    }

    #[test]
    fn async_impl_wraps_with_jsfuture() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    async fn js_save(&self, value: u32) -> Result<(), JsValue>;
                }
            },
        );

        let impl_start = output.find("impl Foo for JsFoo").expect("must have impl");
        let impl_section = &output[impl_start..];

        assert!(
            impl_section.contains("JsFuture"),
            "async impl must use JsFuture.\nImpl: {impl_section}",
        );
        assert!(
            impl_section.contains("unchecked_into"),
            "async impl must use unchecked_into for Err arm.\nImpl: {impl_section}",
        );
    }

    #[test]
    fn unit_ok_type_discards_value() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    async fn js_save(&self) -> Result<(), JsValue>;
                }
            },
        );

        let impl_start = output.find("impl Foo for JsFoo").expect("must have impl");
        let impl_section = &output[impl_start..];

        // For Result<(), JsValue>, the Ok arm should be Ok(()) not Ok(unchecked_into)
        assert!(
            impl_section.contains("Ok (())") || impl_section.contains("Ok(())"),
            "() Ok type must discard value with Ok(()).\nImpl: {impl_section}",
        );
    }

    #[test]
    fn static_method_no_this() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    fn js_create(name: String) -> u32;
                }
            },
        );

        let extern_start = output.find("extern \"C\"").expect("must have extern block");
        let extern_end = output[extern_start..]
            .find("pub trait")
            .map(|i| i + extern_start)
            .unwrap_or(output.len());
        let extern_section = &output[extern_start..extern_end];

        assert!(
            !extern_section.contains("this"),
            "static methods must not have `this` parameter.\nExtern: {extern_section}",
        );
        assert!(
            !extern_section.contains("method"),
            "static methods must not have `method` attr.\nExtern: {extern_section}",
        );
    }

    #[test]
    fn error_on_generic_trait() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo<T> {
                    fn js_put(&self, value: T);
                }
            },
        );

        assert!(
            output.contains("compile_error"),
            "generic traits must produce a compile error.\nOutput: {output}",
        );
    }

    #[test]
    fn error_on_async_without_result() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    async fn js_save(&self, value: u32) -> u32;
                }
            },
        );

        assert!(
            output.contains("compile_error"),
            "async without Result must produce a compile error.\nOutput: {output}",
        );
    }

    #[test]
    fn error_on_mut_self() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    fn js_put(&mut self, value: u32);
                }
            },
        );

        assert!(
            output.contains("compile_error"),
            "&mut self must produce a compile error.\nOutput: {output}",
        );
    }

    #[test]
    fn ts_maps_precise_types() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "save")]
                    async fn js_save(&self, bytes: Uint8Array) -> Result<Uint8Array, JsValue>;
                }
            },
        );

        assert!(
            output.contains("Promise<Uint8Array>"),
            "async Result<Uint8Array, _> must map to Promise<Uint8Array> in TS.\nOutput: {output}",
        );
    }

    #[test]
    fn ts_void_for_unit() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "send")]
                    async fn js_send(&self, data: u32) -> Result<(), JsValue>;
                }
            },
        );

        assert!(
            output.contains("Promise<void>"),
            "async Result<(), _> must map to Promise<void> in TS.\nOutput: {output}",
        );
    }

    #[test]
    fn catch_forwarded_to_extern() {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(catch, js_name = "tryLoad")]
                    fn js_try_load(&self, key: u32) -> Result<JsValue, JsValue>;
                }
            },
        );

        let extern_start = output.find("extern \"C\"").expect("must have extern block");
        let extern_end = output[extern_start..]
            .find("pub trait")
            .map(|i| i + extern_start)
            .unwrap_or(output.len());
        let extern_section = &output[extern_start..extern_end];

        assert!(
            extern_section.contains("catch"),
            "catch must be forwarded to extern fn.\nExtern: {extern_section}",
        );
    }
}
