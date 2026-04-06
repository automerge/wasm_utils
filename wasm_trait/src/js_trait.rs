//! Implementation of the `#[js_trait(js_type = JsTransport)]` attribute macro.
//!
//! Placed on a Rust trait definition, generates:
//! 1. `typescript_custom_section` with a precise TS interface
//! 2. `extern "C"` block with `#[wasm_bindgen]` bindings
//! 3. Rust trait (`wasm_bindgen` attrs stripped, `async fn` preserved)
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
    attrs::{extract_js_name_from_attrs, strip_wasm_bindgen},
    method::{arg_names, has_self_receiver, to_rpitit_signature},
    naming::extern_fn_ident,
    ts_types::{async_return_to_ts, sync_return_to_ts},
};

/// Parsed arguments to `#[js_trait(js_type = JsTransport, js_name = Transport)]`.
pub(crate) struct JsTraitArgs {
    /// Required: Rust ident for the generated extern type.
    pub js_type: Ident,
    /// Optional: JS/TS interface name. Defaults to the trait name.
    pub js_name: Option<String>,
}

impl Parse for JsTraitArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
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

/// Extract the `js_name` value from `wasm_bindgen` attrs on a trait method.
fn extract_js_name(method: &TraitItemFn) -> Option<String> {
    extract_js_name_from_attrs(&method.attrs)
}

/// Collect `wasm_bindgen` meta items from a method's attrs.
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
pub(crate) fn js_trait_impl(args: JsTraitArgs, trait_def: &ItemTrait) -> TokenStream {
    let trait_name = &trait_def.ident;
    let trait_vis = &trait_def.vis;
    let js_type_ident = &args.js_type;
    let ts_interface_name = args.js_name.unwrap_or_else(|| trait_name.to_string());

    if let Some(err) = validate_trait(trait_def) {
        return err;
    }

    let methods: Vec<MethodInfo<'_>> = parse_methods(trait_def);

    if let Some(err) = validate_methods(&methods) {
        return err;
    }

    let ts_section = gen_ts_section(trait_name, &ts_interface_name, &methods);
    let extern_block = gen_extern_block(trait_vis, js_type_ident, &ts_interface_name, &methods);
    let trait_output = gen_trait_def(trait_def, &methods);
    let impl_block = gen_impl_block(trait_name, js_type_ident, &methods);

    quote! {
        #ts_section
        #extern_block
        #trait_output
        #impl_block
    }
}

/// Validate that the trait has no generics, associated types, or associated constants.
fn validate_trait(trait_def: &ItemTrait) -> Option<TokenStream> {
    if !trait_def.generics.params.is_empty() {
        return Some(
            syn::Error::new(
                trait_def.generics.span(),
                "js_trait does not support generic traits",
            )
            .to_compile_error(),
        );
    }

    for item in &trait_def.items {
        match item {
            TraitItem::Type(t) => {
                return Some(
                    syn::Error::new(t.span(), "js_trait does not support associated types")
                        .to_compile_error(),
                );
            }
            TraitItem::Const(c) => {
                return Some(
                    syn::Error::new(c.span(), "js_trait does not support associated constants")
                        .to_compile_error(),
                );
            }
            TraitItem::Fn(_) | TraitItem::Macro(_) | TraitItem::Verbatim(_) | _ => {}
        }
    }

    None
}

/// Parse all trait items into `MethodInfo` records.
fn parse_methods(trait_def: &ItemTrait) -> Vec<MethodInfo<'_>> {
    trait_def
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
        .collect()
}

/// Validate that async methods return `Result` and receivers are `&self` only.
fn validate_methods(methods: &[MethodInfo<'_>]) -> Option<TokenStream> {
    for mi in methods {
        if mi.is_async {
            let returns_result = match &mi.method.sig.output {
                ReturnType::Type(_, ty) => {
                    if let syn::Type::Path(tp) = ty.as_ref() {
                        tp.path
                            .segments
                            .last()
                            .is_some_and(|seg| seg.ident == "Result")
                    } else {
                        false
                    }
                }
                ReturnType::Default => false,
            };

            if !returns_result {
                return Some(
                    syn::Error::new(
                        mi.method.sig.span(),
                        "js_trait async methods must return Result (JS promises can reject)",
                    )
                    .to_compile_error(),
                );
            }
        }

        for arg in &mi.method.sig.inputs {
            if let FnArg::Receiver(recv) = arg {
                if recv.reference.is_none() || recv.mutability.is_some() {
                    return Some(
                        syn::Error::new(
                            recv.span(),
                            "js_trait methods must use `&self` or no receiver",
                        )
                        .to_compile_error(),
                    );
                }
            }
        }

        if mi.js_name.is_none() {
            return Some(
                syn::Error::new(
                    mi.method.sig.ident.span(),
                    "js_trait methods must have #[wasm_bindgen(js_name = \"...\")] \
                     to specify the JS method name",
                )
                .to_compile_error(),
            );
        }

        // Reject method-level generics (wasm_bindgen can't import/export generic fns)
        if !mi.method.sig.generics.params.is_empty() {
            return Some(
                syn::Error::new(
                    mi.method.sig.generics.span(),
                    "js_trait methods cannot have generic parameters \
                     (wasm_bindgen does not support generic functions)",
                )
                .to_compile_error(),
            );
        }
        if mi.method.sig.generics.where_clause.is_some() {
            return Some(
                syn::Error::new(
                    mi.method.sig.generics.where_clause.span(),
                    "js_trait methods cannot have where clauses \
                     (wasm_bindgen does not support generic functions)",
                )
                .to_compile_error(),
            );
        }

        // Reject non-ident arg patterns (destructuring, wildcards, etc.)
        for arg in &mi.method.sig.inputs {
            if let FnArg::Typed(pat_type) = arg {
                if !matches!(pat_type.pat.as_ref(), syn::Pat::Ident(_)) {
                    return Some(
                        syn::Error::new(
                            pat_type.pat.span(),
                            "js_trait method arguments must be simple identifiers, \
                             not destructuring patterns or wildcards",
                        )
                        .to_compile_error(),
                    );
                }
            }
        }
    }

    None
}

/// Generate the `typescript_custom_section` constant.
fn gen_ts_section(
    trait_name: &Ident,
    ts_interface_name: &str,
    methods: &[MethodInfo<'_>],
) -> TokenStream {
    let ts_methods: Vec<String> = methods
        .iter()
        .map(|mi| {
            let default_name = mi.method.sig.ident.to_string();
            let method_name = mi.js_name.as_deref().unwrap_or(&default_name);

            let ts_params: Vec<String> = mi
                .method
                .sig
                .inputs
                .iter()
                .filter_map(|arg| {
                    if let FnArg::Typed(pat_type) = arg {
                        let param_name = match pat_type.pat.as_ref() {
                            syn::Pat::Ident(pi) => pi.ident.to_string(),
                            syn::Pat::Wild(_) | _ => "_".into(),
                        };
                        let ts_type = crate::shared::ts_types::rust_type_to_ts(&pat_type.ty);
                        Some(format!("{param_name}: {ts_type}"))
                    } else {
                        None
                    }
                })
                .collect();

            let params_str = ts_params.join(", ");

            let ts_return = if mi.is_async {
                match &mi.method.sig.output {
                    ReturnType::Type(_, ty) => async_return_to_ts(ty),
                    // Validation rejects async methods without a return type,
                    // but fall back to Promise<void> for robustness.
                    ReturnType::Default => "Promise<void>".into(),
                }
            } else {
                sync_return_to_ts(&mi.method.sig.output)
            };

            format!("    {method_name}({params_str}): {ts_return};")
        })
        .collect();

    let ts_body = ts_methods.join("\n");
    let ts_section_str = format!("export interface {ts_interface_name} {{\n{ts_body}\n}}");

    let ts_const_name = Ident::new(
        &format!(
            "__WASM_TRAIT_TS_{}",
            heck::ToShoutySnakeCase::to_shouty_snake_case(trait_name.to_string().as_str())
        ),
        trait_name.span(),
    );

    quote! {
        #[wasm_bindgen(typescript_custom_section)]
        const #ts_const_name: &str = #ts_section_str;
    }
}

/// Generate the `extern "C"` block with `wasm_bindgen` bindings.
fn gen_extern_block(
    trait_vis: &syn::Visibility,
    js_type_ident: &Ident,
    ts_interface_name: &str,
    methods: &[MethodInfo<'_>],
) -> TokenStream {
    let extern_fns: Vec<TokenStream> = methods
        .iter()
        .map(|mi| {
            let method_name = &mi.method.sig.ident;
            let extern_name = extern_fn_ident(method_name);

            let mut wb_metas: Vec<TokenStream> = Vec::new();

            if mi.has_self {
                wb_metas.push(quote!(method));
            }

            for meta in collect_wb_meta(mi.method) {
                wb_metas.push(quote!(#meta));
            }

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

    quote! {
        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(typescript_type = #ts_interface_name)]
            #trait_vis type #js_type_ident;

            #(#extern_fns)*
        }
    }
}

/// Generate the cleaned-up Rust trait definition.
fn gen_trait_def(trait_def: &ItemTrait, methods: &[MethodInfo<'_>]) -> TokenStream {
    let trait_name = &trait_def.ident;
    let trait_vis = &trait_def.vis;

    let trait_methods: Vec<TokenStream> = methods
        .iter()
        .map(|mi| {
            let clean_attrs = strip_wasm_bindgen(&mi.method.attrs);
            // Keep async fn as-is in the trait (don't transform to RPITIT).
            // async fn in traits is stable since Rust 1.75, and since this
            // is Wasm-only (single-threaded), !Send is fine.
            // Implementors can use either `async fn` or `-> impl Future`.
            let sig = &mi.method.sig;

            quote! {
                #(#clean_attrs)*
                #sig;
            }
        })
        .collect();

    // Build the associated const with expected JS method names.
    // This const is used by #[wasm_implements] to verify the impl block
    // exports all required JS methods. It's an associated const on the
    // trait so it resolves wherever the trait is in scope (even via `use`).
    let js_names: Vec<String> = methods
        .iter()
        .map(|mi| {
            mi.js_name
                .clone()
                .unwrap_or_else(|| extern_fn_ident(&mi.method.sig.ident).to_string())
        })
        .collect();
    let js_name_literals: Vec<_> = js_names.iter().map(|s| quote!(#s)).collect();

    let trait_attrs: Vec<_> = trait_def.attrs.iter().collect();
    let trait_supertraits = &trait_def.supertraits;
    let colon_token = trait_def.colon_token;

    if trait_supertraits.is_empty() {
        quote! {
            #(#trait_attrs)*
            #trait_vis trait #trait_name {
                #[doc(hidden)]
                const __JS_INTERFACE: &[&str] = &[#(#js_name_literals),*];

                #(#trait_methods)*
            }
        }
    } else {
        quote! {
            #(#trait_attrs)*
            #trait_vis trait #trait_name #colon_token #trait_supertraits {
                #[doc(hidden)]
                const __JS_INTERFACE: &[&str] = &[#(#js_name_literals),*];

                #(#trait_methods)*
            }
        }
    }
}

/// Generate the `impl Trait for ExternType` block.
fn gen_impl_block(
    trait_name: &Ident,
    js_type_ident: &Ident,
    methods: &[MethodInfo<'_>],
) -> TokenStream {
    let impl_methods: Vec<TokenStream> = methods
        .iter()
        .map(|mi| {
            let method_name = &mi.method.sig.ident;
            let extern_name = extern_fn_ident(method_name);
            let rpitit_sig = to_rpitit_signature(&mi.method.sig);
            let arg_names: Vec<_> = arg_names(&mi.method.sig);

            if mi.is_async {
                gen_async_impl_method(mi, &extern_name, &rpitit_sig, &arg_names)
            } else {
                // wasm-bindgen `method` fns become methods on the extern type;
                // static fns (no `this`) become free functions.
                let call = if mi.has_self {
                    quote! { self.#extern_name(#(#arg_names),*) }
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

    quote! {
        impl #trait_name for #js_type_ident {
            #(#impl_methods)*
        }
    }
}

/// Generate an async impl method body with `JsFuture` + `unchecked_into`.
fn gen_async_impl_method(
    mi: &MethodInfo<'_>,
    extern_name: &Ident,
    rpitit_sig: &syn::Signature,
    arg_names: &[TokenStream],
) -> TokenStream {
    let extern_call = if mi.has_self {
        quote! { self.#extern_name(#(#arg_names),*) }
    } else {
        quote! { #extern_name(#(#arg_names),*) }
    };

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
    use alloc::{boxed::Box, string::ToString};
    use core::error::Error;
    use quote::quote;

    type TestResult = Result<(), Box<dyn Error>>;

    fn expand(
        attr: proc_macro2::TokenStream,
        item: proc_macro2::TokenStream,
    ) -> Result<String, syn::Error> {
        let args: JsTraitArgs = syn::parse2(attr)?;
        let trait_def: ItemTrait = syn::parse2(item)?;
        Ok(js_trait_impl(args, &trait_def).to_string())
    }

    #[test]
    fn generates_extern_type() -> TestResult {
        let output = expand(
            quote!(js_type = JsTransport),
            quote! {
                pub trait Transport {
                    #[wasm_bindgen(js_name = "name")]
                    fn js_name(&self) -> String;
                }
            },
        )?;

        assert!(
            output.contains("type JsTransport"),
            "must generate extern type JsTransport.\nOutput: {output}",
        );
        assert!(
            output.contains("typescript_type = \"Transport\""),
            "must set typescript_type to trait name.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn generates_js_interface_const() -> TestResult {
        let output = expand(
            quote!(js_type = JsTransport),
            quote! {
                pub trait Transport {
                    #[wasm_bindgen(js_name = "sendBytes")]
                    fn js_send_bytes(&self, bytes: u32);

                    #[wasm_bindgen(js_name = "recvBytes")]
                    fn js_recv_bytes(&self) -> u32;
                }
            },
        )?;

        assert!(
            output.contains("__JS_INTERFACE"),
            "trait must contain the associated __JS_INTERFACE const.\nOutput: {output}",
        );
        assert!(
            output.contains(r#""sendBytes""#),
            "const must contain sendBytes.\nOutput: {output}",
        );
        assert!(
            output.contains(r#""recvBytes""#),
            "const must contain recvBytes.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn error_on_missing_js_name() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    fn js_bare_method(&self);
                }
            },
        )?;

        assert!(
            output.contains("compile_error"),
            "missing js_name must produce a compile error.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn generates_typescript_interface() -> TestResult {
        let output = expand(
            quote!(js_type = JsTransport),
            quote! {
                pub trait Transport {
                    #[wasm_bindgen(js_name = "getName")]
                    fn js_name(&self) -> String;
                }
            },
        )?;

        assert!(
            output.contains("export interface Transport"),
            "must generate TS interface named Transport.\nOutput: {output}",
        );
        assert!(
            output.contains("getName"),
            "must use js_name in TS interface.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn js_name_override() -> TestResult {
        let output = expand(
            quote!(js_type = JsTransport, js_name = MyTransport),
            quote! {
                pub trait Transport {
                    #[wasm_bindgen(js_name = "name")]
                    fn js_name(&self) -> String;
                }
            },
        )?;

        assert!(
            output.contains("export interface MyTransport"),
            "must use js_name override for TS interface.\nOutput: {output}",
        );
        assert!(
            output.contains("typescript_type = \"MyTransport\""),
            "must use js_name override for typescript_type.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn generates_trait_without_wasm_bindgen_attrs() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "put")]
                    fn js_put(&self, value: u32) -> u32;
                }
            },
        )?;

        // Find the trait definition (not in extern block, not in impl block)
        assert!(
            output.contains("pub trait Foo"),
            "must emit the Rust trait.\nOutput: {output}",
        );

        // The trait shouldn't have wasm_bindgen attrs on methods
        // Find the trait section
        let trait_start = output.find("pub trait Foo").ok_or("must have trait")?;
        // Find next `impl` or `const` after the trait (end of trait block)
        let trait_end = output[trait_start..]
            .find("impl Foo for JsFoo")
            .map_or(output.len(), |i| i + trait_start);
        let trait_section = &output[trait_start..trait_end];

        assert!(
            !trait_section.contains("wasm_bindgen"),
            "trait must not contain wasm_bindgen attrs.\nTrait section: {trait_section}",
        );
        Ok(())
    }

    #[test]
    fn generates_impl_for_extern_type() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "put")]
                    fn js_put(&self, value: u32) -> u32;
                }
            },
        )?;

        assert!(
            output.contains("impl Foo for JsFoo"),
            "must generate impl Trait for ExternType.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn extern_fns_prefixed() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "put")]
                    fn js_put(&self, value: u32) -> u32;
                }
            },
        )?;

        assert!(
            output.contains("__wasm_trait_js_put"),
            "extern fns must be prefixed with __wasm_trait_.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn async_method_returns_promise_in_extern() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "save")]
                    async fn js_save(&self, value: u32) -> Result<(), JsValue>;
                }
            },
        )?;

        // Extern block should have Promise return
        let extern_start = output
            .find("extern \"C\"")
            .ok_or("must have extern block")?;
        let extern_end = output[extern_start..]
            .find("pub trait")
            .map_or(output.len(), |i| i + extern_start);
        let extern_section = &output[extern_start..extern_end];

        assert!(
            extern_section.contains("Promise"),
            "async extern fns must return Promise.\nExtern: {extern_section}",
        );
        Ok(())
    }

    #[test]
    fn async_method_stays_async_in_trait() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "save")]
                    async fn js_save(&self, value: u32) -> Result<(), JsValue>;
                }
            },
        )?;

        let trait_start = output.find("pub trait Foo").ok_or("must have trait")?;
        let trait_end = output[trait_start..]
            .find("impl Foo for JsFoo")
            .map_or(output.len(), |i| i + trait_start);
        let trait_section = &output[trait_start..trait_end];

        assert!(
            trait_section.contains("async"),
            "async trait methods must keep async fn in trait.\nTrait: {trait_section}",
        );
        assert!(
            !trait_section.contains("Future"),
            "trait must use async fn, not RPITIT.\nTrait: {trait_section}",
        );
        Ok(())
    }

    #[test]
    fn async_impl_wraps_with_jsfuture() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "save")]
                    async fn js_save(&self, value: u32) -> Result<(), JsValue>;
                }
            },
        )?;

        let impl_start = output.find("impl Foo for JsFoo").ok_or("must have impl")?;
        let impl_section = &output[impl_start..];

        assert!(
            impl_section.contains("JsFuture"),
            "async impl must use JsFuture.\nImpl: {impl_section}",
        );
        assert!(
            impl_section.contains("unchecked_into"),
            "async impl must use unchecked_into for Err arm.\nImpl: {impl_section}",
        );
        Ok(())
    }

    #[test]
    fn unit_ok_type_discards_value() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "save")]
                    async fn js_save(&self) -> Result<(), JsValue>;
                }
            },
        )?;

        let impl_start = output.find("impl Foo for JsFoo").ok_or("must have impl")?;
        let impl_section = &output[impl_start..];

        // For Result<(), JsValue>, the Ok arm should be Ok(()) not Ok(unchecked_into)
        assert!(
            impl_section.contains("Ok (())") || impl_section.contains("Ok(())"),
            "() Ok type must discard value with Ok(()).\nImpl: {impl_section}",
        );
        Ok(())
    }

    #[test]
    fn static_method_no_this() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "create")]
                    fn js_create(name: String) -> u32;
                }
            },
        )?;

        let extern_start = output
            .find("extern \"C\"")
            .ok_or("must have extern block")?;
        let extern_end = output[extern_start..]
            .find("pub trait")
            .map_or(output.len(), |i| i + extern_start);
        let extern_section = &output[extern_start..extern_end];

        assert!(
            !extern_section.contains("this"),
            "static methods must not have `this` parameter.\nExtern: {extern_section}",
        );
        assert!(
            !extern_section.contains("method"),
            "static methods must not have `method` attr.\nExtern: {extern_section}",
        );
        Ok(())
    }

    #[test]
    fn error_on_generic_trait() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo<T> {
                    fn js_put(&self, value: T);
                }
            },
        )?;

        assert!(
            output.contains("compile_error"),
            "generic traits must produce a compile error.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn error_on_async_without_result() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    async fn js_save(&self, value: u32) -> u32;
                }
            },
        )?;

        assert!(
            output.contains("compile_error"),
            "async without Result must produce a compile error.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn error_on_mut_self() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    fn js_put(&mut self, value: u32);
                }
            },
        )?;

        assert!(
            output.contains("compile_error"),
            "&mut self must produce a compile error.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn error_on_owned_self() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    fn js_put(self, value: u32);
                }
            },
        )?;

        assert!(
            output.contains("compile_error"),
            "owned self must produce a compile error.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn error_on_generic_method() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "put")]
                    fn js_put<T>(&self, value: T);
                }
            },
        )?;

        assert!(
            output.contains("compile_error"),
            "generic methods must produce a compile error.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn error_on_method_where_clause() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "put")]
                    fn js_put(&self, value: u32) where Self: Clone;
                }
            },
        )?;

        assert!(
            output.contains("compile_error"),
            "method where clauses must produce a compile error.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn error_on_destructuring_pattern() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "put")]
                    fn js_put(&self, (a, b): (u32, u32));
                }
            },
        )?;

        assert!(
            output.contains("compile_error"),
            "destructuring patterns must produce a compile error.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn ts_maps_precise_types() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "save")]
                    async fn js_save(&self, bytes: Uint8Array) -> Result<Uint8Array, JsValue>;
                }
            },
        )?;

        assert!(
            output.contains("Promise<Uint8Array>"),
            "async Result<Uint8Array, _> must map to Promise<Uint8Array> in TS.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn ts_void_for_unit() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "send")]
                    async fn js_send(&self, data: u32) -> Result<(), JsValue>;
                }
            },
        )?;

        assert!(
            output.contains("Promise<void>"),
            "async Result<(), _> must map to Promise<void> in TS.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn catch_forwarded_to_extern() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(catch, js_name = "tryLoad")]
                    fn js_try_load(&self, key: u32) -> Result<JsValue, JsValue>;
                }
            },
        )?;

        let extern_start = output
            .find("extern \"C\"")
            .ok_or("must have extern block")?;
        let extern_end = output[extern_start..]
            .find("pub trait")
            .map_or(output.len(), |i| i + extern_start);
        let extern_section = &output[extern_start..extern_end];

        assert!(
            extern_section.contains("catch"),
            "catch must be forwarded to extern fn.\nExtern: {extern_section}",
        );
        Ok(())
    }

    /// Extract the substring from `start` to `end` (exclusive).
    #[allow(clippy::panic)]
    fn between<'a>(output: &'a str, start: &str, end: &str) -> &'a str {
        let s = output
            .find(start)
            .unwrap_or_else(|| panic!("start marker not found: {start}"));
        let e = output[s..].find(end).map_or(output.len(), |i| i + s);
        &output[s..e]
    }

    /// Extract from `start` to the end of the string.
    #[allow(clippy::panic)]
    fn from<'a>(output: &'a str, start: &str) -> &'a str {
        let s = output
            .find(start)
            .unwrap_or_else(|| panic!("start marker not found: {start}"));
        &output[s..]
    }

    // ------------------------------------------------------------------
    // Full expansion inspection tests
    // ------------------------------------------------------------------

    #[test]
    fn full_sync_expansion() -> TestResult {
        let output = expand(
            quote!(js_type = JsCounter),
            quote! {
                pub trait Counter {
                    #[wasm_bindgen(js_name = "getCount")]
                    fn get_count(&self) -> u32;

                    #[wasm_bindgen(js_name = "setCount")]
                    fn set_count(&self, value: u32);
                }
            },
        )?;

        // -- TS section --
        let ts = between(&output, "__WASM_TRAIT_TS_COUNTER", "extern \"C\"");
        assert!(
            ts.contains("export interface Counter"),
            "TS must declare interface Counter.\nTS: {ts}",
        );
        assert!(
            ts.contains("getCount(): number;"),
            "TS must map u32 → number for getCount.\nTS: {ts}",
        );
        assert!(
            ts.contains("setCount(value: number): void;"),
            "TS must map setCount(u32) → void.\nTS: {ts}",
        );

        // -- Extern block --
        let ext = between(&output, "extern \"C\"", "pub trait Counter");
        assert!(
            ext.contains("typescript_type = \"Counter\""),
            "extern type must set typescript_type.\nExtern: {ext}",
        );
        assert!(
            ext.contains("type JsCounter"),
            "extern must declare JsCounter type.\nExtern: {ext}",
        );
        // Both methods are instance methods → must have `method` attr
        assert!(
            ext.contains("method") && ext.contains("fn __wasm_trait_get_count"),
            "get_count extern must have method attr.\nExtern: {ext}",
        );
        assert!(
            ext.contains("method") && ext.contains("fn __wasm_trait_set_count"),
            "set_count extern must have method attr.\nExtern: {ext}",
        );
        // Both must have `this` parameter
        assert!(
            ext.contains("this : & JsCounter"),
            "instance extern fns must have `this` param.\nExtern: {ext}",
        );

        // -- Trait definition --
        let tr = between(&output, "pub trait Counter", "impl Counter for JsCounter");
        assert!(
            !tr.contains("wasm_bindgen"),
            "trait must not contain wasm_bindgen.\nTrait: {tr}",
        );
        assert!(
            tr.contains("fn get_count (& self) -> u32"),
            "trait must declare get_count with &self.\nTrait: {tr}",
        );
        assert!(
            tr.contains("fn set_count (& self , value : u32)"),
            "trait must declare set_count with &self.\nTrait: {tr}",
        );

        // -- Impl block --
        let imp = from(&output, "impl Counter for JsCounter");
        assert!(
            imp.contains("self . __wasm_trait_get_count ()"),
            "impl must delegate via self.method() syntax, not free fn.\nImpl: {imp}",
        );
        assert!(
            imp.contains("self . __wasm_trait_set_count (value)"),
            "impl must delegate set_count via self.method() syntax.\nImpl: {imp}",
        );
        Ok(())
    }

    #[test]
    fn full_async_expansion() -> TestResult {
        let output = expand(
            quote!(js_type = JsStore),
            quote! {
                pub trait Store {
                    #[wasm_bindgen(js_name = "save")]
                    async fn js_save(&self, data: Uint8Array) -> Result<Uint8Array, JsValue>;
                }
            },
        )?;

        // -- TS section --
        let ts = between(&output, "__WASM_TRAIT_TS_STORE", "extern \"C\"");
        assert!(
            ts.contains("save(data: Uint8Array): Promise<Uint8Array>;"),
            "TS must use js_name and map async Result<Uint8Array> → Promise<Uint8Array>.\nTS: {ts}",
        );

        // -- Extern block --
        let ext = between(&output, "extern \"C\"", "pub trait Store");
        assert!(
            ext.contains("js_name = \"save\""),
            "extern must forward js_name.\nExtern: {ext}",
        );
        assert!(
            ext.contains("-> :: js_sys :: Promise"),
            "async extern fn must return Promise, not Result.\nExtern: {ext}",
        );
        assert!(
            !ext.contains("Result"),
            "extern fn must not mention Result.\nExtern: {ext}",
        );
        assert!(
            ext.contains("method"),
            "instance extern fn must have method attr.\nExtern: {ext}",
        );

        // -- Trait definition (async fn, not RPITIT) --
        let tr = between(&output, "pub trait Store", "impl Store for JsStore");
        assert!(
            tr.contains("async"),
            "trait must keep async fn.\nTrait: {tr}",
        );
        assert!(
            !tr.contains("impl :: core :: future :: Future"),
            "trait must use async fn, not RPITIT.\nTrait: {tr}",
        );
        assert!(
            tr.contains("Result < Uint8Array , JsValue >"),
            "async trait method must have the original Result return type.\nTrait: {tr}",
        );

        // -- Impl block --
        let imp = from(&output, "impl Store for JsStore");
        assert!(
            imp.contains("JsFuture :: from (__promise)"),
            "impl must use JsFuture.\nImpl: {imp}",
        );
        assert!(
            imp.contains("unchecked_into (__v)"),
            "impl Ok arm must use unchecked_into for non-unit type.\nImpl: {imp}",
        );
        assert!(
            imp.contains("unchecked_into (__e)"),
            "impl Err arm must use unchecked_into.\nImpl: {imp}",
        );
        assert!(
            imp.contains("self . __wasm_trait_js_save (data)"),
            "impl must call extern fn via self.method() syntax.\nImpl: {imp}",
        );
        Ok(())
    }

    #[test]
    fn full_void_async_expansion() -> TestResult {
        let output = expand(
            quote!(js_type = JsSender),
            quote! {
                pub trait Sender {
                    #[wasm_bindgen(js_name = "send")]
                    async fn send(&self, data: u32) -> Result<(), JsValue>;
                }
            },
        )?;

        // -- TS section --
        let ts = between(&output, "__WASM_TRAIT_TS_SENDER", "extern \"C\"");
        assert!(
            ts.contains("Promise<void>"),
            "async Result<()> must map to Promise<void> in TS.\nTS: {ts}",
        );

        // -- Impl block: Ok arm must be Ok(()), NOT unchecked_into --
        let imp = from(&output, "impl Sender for JsSender");
        let ok_arm = between(
            imp,
            ":: core :: result :: Result :: Ok",
            ":: core :: result :: Result :: Err",
        );
        assert!(
            ok_arm.contains("Ok (())"),
            "void Ok arm must discard the value with Ok(()).\nOk arm: {ok_arm}",
        );
        assert!(
            !ok_arm.contains("unchecked_into"),
            "void Ok arm must NOT use unchecked_into.\nOk arm: {ok_arm}",
        );

        // Err arm should still use unchecked_into
        let err_arm = from(imp, ":: core :: result :: Result :: Err");
        assert!(
            err_arm.contains("unchecked_into (__e)"),
            "Err arm must still use unchecked_into.\nErr arm: {err_arm}",
        );
        Ok(())
    }

    #[test]
    fn full_static_expansion() -> TestResult {
        let output = expand(
            quote!(js_type = JsFactory),
            quote! {
                pub trait Factory {
                    #[wasm_bindgen(js_name = "create")]
                    fn create(name: String) -> u32;
                }
            },
        )?;

        // -- Extern block --
        let ext = between(&output, "extern \"C\"", "pub trait Factory");

        // The extern fn for a static method must NOT have `method` attr
        // Note: the macro currently emits `#[wasm_bindgen()]` with empty parens
        // for static methods — no `method` keyword should be present.
        let fn_section = between(ext, "fn __wasm_trait_create", ";");
        assert!(
            !fn_section.contains("method"),
            "static extern fn must not have `method` attr.\nFn: {fn_section}",
        );
        assert!(
            !ext.contains("this"),
            "static extern fn must not have `this` parameter.\nExtern: {ext}",
        );

        // -- Trait definition --
        let tr = between(&output, "pub trait Factory", "impl Factory for JsFactory");
        assert!(
            !tr.contains("& self"),
            "static trait method must not have &self.\nTrait: {tr}",
        );

        // -- Impl block --
        let imp = from(&output, "impl Factory for JsFactory");
        assert!(
            !imp.contains("self .") && !imp.contains("self."),
            "static impl must not call via self.\nImpl: {imp}",
        );
        assert!(
            imp.contains("__wasm_trait_create (name)"),
            "static impl must call free fn directly.\nImpl: {imp}",
        );
        Ok(())
    }

    #[test]
    fn full_mixed_expansion() -> TestResult {
        let output = expand(
            quote!(js_type = JsService),
            quote! {
                pub trait Service {
                    #[wasm_bindgen(js_name = "name")]
                    fn name(&self) -> String;

                    #[wasm_bindgen(js_name = "fetch")]
                    async fn fetch(&self, key: u32) -> Result<Uint8Array, JsValue>;

                    #[wasm_bindgen(js_name = "create")]
                    fn create(label: String) -> bool;
                }
            },
        )?;

        // -- TS section: all three methods present --
        let ts = between(&output, "__WASM_TRAIT_TS_SERVICE", "extern \"C\"");
        assert!(
            ts.contains("name(): string;"),
            "TS must have sync name() method.\nTS: {ts}",
        );
        assert!(
            ts.contains("fetch(key: number): Promise<Uint8Array>;"),
            "TS must have async fetch() method.\nTS: {ts}",
        );
        assert!(
            ts.contains("create(label: string): boolean;"),
            "TS must have static create() method.\nTS: {ts}",
        );

        // -- Extern block structure --
        let ext = between(&output, "extern \"C\"", "pub trait Service");

        // `name` is instance → method attr + this
        assert!(
            ext.contains("method") && ext.contains("fn __wasm_trait_name (this : & JsService)"),
            "name extern must be method with this.\nExtern: {ext}",
        );

        // `fetch` is instance + async → method attr + this + returns Promise
        assert!(
            ext.contains("method") && ext.contains("fn __wasm_trait_fetch (this : & JsService"),
            "fetch extern must be method with this.\nExtern: {ext}",
        );
        assert!(
            ext.contains(
                "__wasm_trait_fetch (this : & JsService , key : u32) -> :: js_sys :: Promise"
            ),
            "fetch extern must return Promise.\nExtern: {ext}",
        );

        // `create` is static → no method, no this
        let create_fn_section = between(ext, "fn __wasm_trait_create", ";");
        assert!(
            !create_fn_section.contains("method"),
            "create (static) extern must not have method attr.\nFn: {create_fn_section}",
        );
        assert!(
            !create_fn_section.contains("this"),
            "create (static) extern must not have this.\nFn: {create_fn_section}",
        );

        // -- Trait definition --
        let tr = between(&output, "pub trait Service", "impl Service for JsService");
        // sync instance: plain signature
        assert!(
            tr.contains("fn name (& self) -> String"),
            "trait must have name with &self.\nTrait: {tr}",
        );
        // async instance: async fn (not RPITIT)
        assert!(
            tr.contains("async fn fetch (& self , key : u32) -> Result < Uint8Array , JsValue >"),
            "trait must have fetch as async fn.\nTrait: {tr}",
        );
        // static: no &self
        assert!(
            tr.contains("fn create (label : String) -> bool"),
            "trait must have static create.\nTrait: {tr}",
        );

        // -- Impl block --
        let imp = from(&output, "impl Service for JsService");
        // sync instance delegates via self.
        assert!(
            imp.contains("self . __wasm_trait_name ()"),
            "sync instance impl must use self.method().\nImpl: {imp}",
        );
        // async instance uses JsFuture + unchecked_into
        assert!(
            imp.contains("self . __wasm_trait_fetch (key)"),
            "async instance impl must use self.method().\nImpl: {imp}",
        );
        assert!(
            imp.contains("JsFuture :: from (__promise)"),
            "async impl must use JsFuture.\nImpl: {imp}",
        );
        assert!(
            imp.contains("unchecked_into (__v)"),
            "async impl Ok arm must unchecked_into for Uint8Array.\nImpl: {imp}",
        );
        // static delegates via free fn
        assert!(
            imp.contains("{ __wasm_trait_create (label) }"),
            "static impl must call free fn.\nImpl: {imp}",
        );
        Ok(())
    }

    // ------------------------------------------------------------------
    // TS type mapping stress tests (QA round 2)
    // ------------------------------------------------------------------

    #[test]
    fn ts_option_string_maps_to_string_or_null() -> TestResult {
        let output = expand(
            quote!(js_type = JsTsOpt),
            quote! {
                pub trait TsOpt {
                    #[wasm_bindgen(js_name = "maybeName")]
                    fn js_maybe_name(&self) -> Option<String>;
                }
            },
        )?;

        assert!(
            output.contains("string | null"),
            "Option<String> must map to 'string | null' in TS.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn ts_async_result_unit_maps_to_promise_void() -> TestResult {
        let output = expand(
            quote!(js_type = JsTsVoid),
            quote! {
                pub trait TsVoid {
                    #[wasm_bindgen(js_name = "send")]
                    async fn js_send(&self) -> Result<(), JsValue>;
                }
            },
        )?;

        assert!(
            output.contains("Promise<void>"),
            "Result<(), JsValue> in async must map to 'Promise<void>'.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn ts_async_result_array_maps_to_promise_array() -> TestResult {
        let output = expand(
            quote!(js_type = JsTsArr),
            quote! {
                pub trait TsArr {
                    #[wasm_bindgen(js_name = "fetchAll")]
                    async fn js_fetch_all(&self) -> Result<Array, JsValue>;
                }
            },
        )?;

        assert!(
            output.contains("Promise<Array<any>>"),
            "Result<Array, JsValue> in async must map to 'Promise<Array<any>>'.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn ts_bool_return_maps_to_boolean() -> TestResult {
        let output = expand(
            quote!(js_type = JsTsBool),
            quote! {
                pub trait TsBool {
                    #[wasm_bindgen(js_name = "isReady")]
                    fn js_is_ready(&self) -> bool;
                }
            },
        )?;

        assert!(
            output.contains("boolean"),
            "bool return must map to 'boolean' in TS.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn ts_no_return_maps_to_void() -> TestResult {
        let output = expand(
            quote!(js_type = JsTsNoRet),
            quote! {
                pub trait TsNoRet {
                    #[wasm_bindgen(js_name = "reset")]
                    fn js_reset(&self);
                }
            },
        )?;

        assert!(
            output.contains("reset(): void;"),
            "Method with no return type must map to 'void' in TS.\nOutput: {output}",
        );
        Ok(())
    }
}
