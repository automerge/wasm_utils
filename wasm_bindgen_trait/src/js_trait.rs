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
    ext::IdentExt,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    FnArg, ItemTrait, ReturnType, Token, TraitItem, TraitItemFn,
};

use crate::shared::{
    attrs::{extract_js_name_from_attrs, strip_wasm_bindgen},
    method::{arg_names, has_self_receiver, to_rpitit_signature},
    naming::extern_fn_ident,
    ts_names::{binding_name_error, interface_name_error, ts_member_name, ts_param_names},
    ts_types::{async_return_to_ts, rust_type_to_ts, sync_return_to_ts},
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
                    let parsed = parse_js_name_value(&pair.value)?;
                    if let Some(msg) = interface_name_error(&parsed) {
                        return Err(syn::Error::new(pair.value.span(), msg));
                    }
                    js_name = Some(parsed);
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

/// The value of `js_name = ...`: a bare identifier or a string literal.
fn parse_js_name_value(value: &syn::Expr) -> syn::Result<String> {
    if let syn::Expr::Path(expr_path) = value {
        return expr_path
            .path
            .get_ident()
            .map(ToString::to_string)
            .ok_or_else(|| syn::Error::new(value.span(), "js_name must be a simple identifier"));
    }
    if let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(s),
        ..
    }) = value
    {
        return Ok(s.value());
    }
    Err(syn::Error::new(
        value.span(),
        "js_name must be an identifier or string literal",
    ))
}

/// Parsed info about a single trait method.
struct MethodInfo<'a> {
    method: &'a TraitItemFn,
    is_async: bool,
    has_self: bool,
    /// Required on every `#[js_trait]` method; enforced by [`parse_methods`].
    js_name: String,
}

/// Check whether a trait method has `#[wasm_bindgen(catch)]`.
fn has_catch_attr(method: &TraitItemFn) -> bool {
    for attr in &method.attrs {
        if !attr.path().is_ident("wasm_bindgen") {
            continue;
        }
        if let Ok(nested) =
            attr.parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)
        {
            for meta in &nested {
                if let syn::Meta::Path(p) = meta {
                    if p.is_ident("catch") {
                        return true;
                    }
                }
            }
        }
    }
    false
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
    let args_js_name_is_default = args.js_name.is_none();
    let ts_interface_name = args
        .js_name
        .unwrap_or_else(|| trait_name.unraw().to_string());

    if let Some(err) = validate_trait(trait_def) {
        return err;
    }

    let methods: Vec<MethodInfo<'_>> = match parse_methods(trait_def) {
        Ok(methods) => methods,
        Err(err) => return err.to_compile_error(),
    };

    if let Some(err) = validate_methods(&methods) {
        return err;
    }

    if args_js_name_is_default {
        if let Some(msg) = interface_name_error(&ts_interface_name) {
            return syn::Error::new(trait_name.span(), msg).to_compile_error();
        }
    }

    let ts_section = match gen_ts_section(trait_name, &ts_interface_name, &methods) {
        Ok(tokens) => tokens,
        Err(err) => return err.to_compile_error(),
    };
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

/// Parse all trait methods into `MethodInfo` records.
///
/// Fails if any method lacks `#[wasm_bindgen(js_name = ...)]`, so every
/// `MethodInfo` carries its JS name.
fn parse_methods(trait_def: &ItemTrait) -> syn::Result<Vec<MethodInfo<'_>>> {
    trait_def
        .items
        .iter()
        .filter_map(|item| match item {
            TraitItem::Fn(method) => Some(method),
            TraitItem::Const(_)
            | TraitItem::Type(_)
            | TraitItem::Macro(_)
            | TraitItem::Verbatim(_)
            | _ => None,
        })
        .map(|method| {
            let js_name = extract_js_name_from_attrs(&method.attrs).ok_or_else(|| {
                syn::Error::new(
                    method.sig.ident.span(),
                    "js_trait methods must have #[wasm_bindgen(js_name = \"...\")] \
                     to specify the JS method name",
                )
            })?;

            Ok(MethodInfo {
                method,
                is_async: method.sig.asyncness.is_some(),
                has_self: has_self_receiver(&method.sig),
                js_name,
            })
        })
        .collect()
}

/// Reject method shapes `wasm_bindgen` can't bind: default bodies, generic
/// parameters, and where clauses.
fn validate_shape(mi: &MethodInfo<'_>) -> Option<TokenStream> {
    // A default would be silently unreachable: the extern impl always
    // delegates to JS, and `#[wasm_implements]` requires every method to
    // be exported, so the body could never be inherited.
    if let Some(body) = &mi.method.default {
        return Some(
            syn::Error::new(
                body.span(),
                "js_trait methods cannot have default bodies \
                 (every method is a required method of the JS interface)",
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
    None
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

            // Reject catch on async methods — catch is for synchronous JS throws,
            // async methods handle errors via Promise rejection instead.
            if has_catch_attr(mi.method) {
                return Some(
                    syn::Error::new(
                        mi.method.sig.span(),
                        "js_trait async methods cannot use `catch` — \
                         async methods handle JS errors via Promise rejection, \
                         not synchronous throw. Remove `catch` from the \
                         #[wasm_bindgen] attribute.",
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

        if let Some(err) = validate_shape(mi) {
            return Some(err);
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
) -> syn::Result<TokenStream> {
    let mut members: Vec<String> = Vec::new();
    let mut globals: Vec<String> = Vec::new();
    let mut namespaced: Vec<(String, String)> = Vec::new();

    for mi in methods {
        let (params, ret) = ts_signature(mi);

        if mi.has_self {
            let name = ts_member_name(&mi.js_name);
            members.push(format!("    {name}({params}): {ret};"));
            continue;
        }

        // A static method binds to a JS function, not to a member of the
        // interface: declare it where wasm-bindgen will look it up.
        let namespace = match static_binding(mi.method) {
            StaticBinding::Module => continue,
            StaticBinding::Global => None,
            StaticBinding::Namespace(path) => Some(path),
        };
        if let Some(msg) = binding_name_error(&mi.js_name) {
            return Err(syn::Error::new(mi.method.sig.ident.span(), msg));
        }
        let decl = format!("var {}: ({params}) => {ret};", mi.js_name);
        match namespace {
            Some(path) => namespaced.push((path, decl)),
            None => globals.push(decl),
        }
    }

    let mut ts = format!(
        "export interface {ts_interface_name} {{\n{}\n}}",
        members.join("\n")
    );
    if !globals.is_empty() || !namespaced.is_empty() {
        ts.push_str("\ndeclare global {");
        for decl in &globals {
            ts.push_str("\n    ");
            ts.push_str(decl);
        }
        for (path, decl) in &namespaced {
            ts.push_str("\n    namespace ");
            ts.push_str(path);
            ts.push_str(" {\n        ");
            ts.push_str(decl);
            ts.push_str("\n    }");
        }
        ts.push_str("\n}");
    }

    let ts_const_name = Ident::new(
        &format!(
            "__WASM_TRAIT_TS_{}",
            heck::ToShoutySnakeCase::to_shouty_snake_case(trait_name.unraw().to_string().as_str())
        ),
        trait_name.span(),
    );

    Ok(quote! {
        #[::wasm_bindgen::prelude::wasm_bindgen(typescript_custom_section)]
        const #ts_const_name: &str = #ts;
    })
}

/// A method's TS parameter list and return type.
fn ts_signature(mi: &MethodInfo<'_>) -> (String, String) {
    let typed: Vec<&syn::PatType> = mi
        .method
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pat_type) => Some(pat_type),
            FnArg::Receiver(_) => None,
        })
        .collect();

    // Validation guarantees every argument pattern is a plain identifier.
    let idents = typed.iter().filter_map(|pt| {
        if let syn::Pat::Ident(pi) = pt.pat.as_ref() {
            Some(&pi.ident)
        } else {
            None
        }
    });
    let params = ts_param_names(idents)
        .into_iter()
        .zip(&typed)
        .map(|(name, pt)| format!("{name}: {}", rust_type_to_ts(&pt.ty)))
        .collect::<Vec<_>>()
        .join(", ");

    let ret = if mi.is_async {
        match &mi.method.sig.output {
            ReturnType::Type(_, ty) => async_return_to_ts(ty),
            // Validation rejects async methods without a return type.
            ReturnType::Default => "Promise<void>".into(),
        }
    } else {
        sync_return_to_ts(&mi.method.sig.output)
    };

    (params, ret)
}

/// Where wasm-bindgen looks up a static (no-receiver) method's JS function.
enum StaticBinding {
    /// A global function (no namespace attrs).
    Global,
    /// `js_namespace = ...` or `static_method_of = ...`: a property of that
    /// object, written as a dotted TS namespace path.
    Namespace(String),
    /// `module = "..."` / `raw_module = "..."`: an export of a JS module,
    /// which carries its own types, so nothing is declared here.
    Module,
}

fn static_binding(method: &TraitItemFn) -> StaticBinding {
    for meta in collect_wb_meta(method) {
        let syn::Meta::NameValue(nv) = meta else {
            continue;
        };
        if nv.path.is_ident("module") || nv.path.is_ident("raw_module") {
            return StaticBinding::Module;
        }
        if nv.path.is_ident("js_namespace") || nv.path.is_ident("static_method_of") {
            if let Some(path) = namespace_path(&nv.value) {
                return StaticBinding::Namespace(path);
            }
        }
    }
    StaticBinding::Global
}

/// `ns`, `"ns"`, or `["a", "b"]` as a dotted path (`a.b`).
fn namespace_path(value: &syn::Expr) -> Option<String> {
    let segment = |expr: &syn::Expr| -> Option<String> {
        if let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) = expr
        {
            return Some(s.value());
        }
        if let syn::Expr::Path(p) = expr {
            return p.path.get_ident().map(ToString::to_string);
        }
        None
    };
    if let syn::Expr::Array(array) = value {
        let parts: Option<Vec<String>> = array.elems.iter().map(segment).collect();
        return parts.map(|parts| parts.join("."));
    }
    segment(value)
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
                #[::wasm_bindgen::prelude::wasm_bindgen(#(#wb_metas),*)]
                fn #extern_name(#(#params),*) #ret;
            }
        })
        .collect();

    quote! {
        #[::wasm_bindgen::prelude::wasm_bindgen]
        extern "C" {
            #[::wasm_bindgen::prelude::wasm_bindgen(typescript_type = #ts_interface_name)]
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

    // Per-method (Rust name, required JS name, error message) rows. The
    // message is prebuilt here because only this macro knows the trait's JS
    // names; `#[wasm_implements]` looks rows up by Rust name and panics with
    // the message (at compile time) on a mismatch.
    let trait_display = trait_name.unraw();
    let method_rows: Vec<TokenStream> = methods
        .iter()
        .map(|mi| {
            let js_name = &mi.js_name;
            let rust_name = mi.method.sig.ident.unraw().to_string();
            let message = format!(
                "`{trait_display}::{rust_name}` must be exported to JS as \"{js_name}\"; \
                 add #[wasm_bindgen(js_name = \"{js_name}\")] to it in the \
                 #[wasm_implements({trait_display})] impl block"
            );
            quote!((#rust_name, #js_name, #message))
        })
        .collect();

    let trait_attrs: Vec<_> = trait_def.attrs.iter().collect();
    let trait_supertraits = &trait_def.supertraits;
    let colon_token = trait_def.colon_token;

    // Suppress the `async_fn_in_trait` warning when the trait has async
    // methods. The warning is about missing `Send` bounds on the returned
    // future, which is irrelevant in Wasm (single-threaded runtime).
    let has_async = methods.iter().any(|mi| mi.is_async);
    let async_allow = if has_async {
        quote!(#[allow(async_fn_in_trait)])
    } else {
        quote!()
    };

    // Without supertraits, `colon_token` is `None` and both print nothing.
    quote! {
        #(#trait_attrs)*
        #async_allow
        #trait_vis trait #trait_name #colon_token #trait_supertraits {
            #[doc(hidden)]
            const __JS_METHODS: &[(&str, &str, &str)] = &[#(#method_rows),*];

            #(#trait_methods)*
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

    let ok_ty = extract_ok_type(&mi.method.sig.output);

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

/// Extract `T` from a `Result<T, E>` return type (`()` if there is none).
///
/// The error type isn't needed: the generated impl converts the rejection
/// with `unchecked_into`, inferring `E` from the signature.
fn extract_ok_type(ret: &ReturnType) -> TokenStream {
    if let ReturnType::Type(_, ty) = ret {
        if let syn::Type::Path(tp) = ty.as_ref() {
            if let Some(seg) = tp.path.segments.last() {
                if seg.ident == "Result" {
                    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                        if let Some(ok) = args.args.first() {
                            return quote!(#ok);
                        }
                    }
                }
            }
        }
    }
    quote!(())
}

/// Check if a token stream represents the unit type `()`.
fn is_unit_type(ty: &TokenStream) -> bool {
    ty.to_string() == "()"
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
            output.contains("js_trait methods must have #[wasm_bindgen(js_name ="),
            "missing js_name must produce a compile error.\nOutput: {output}",
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
    fn error_on_generic_trait() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo<T> {
                    #[wasm_bindgen(js_name = "put")]
                    fn js_put(&self, value: T);
                }
            },
        )?;

        assert!(
            output.contains("js_trait does not support generic traits"),
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
                    #[wasm_bindgen(js_name = "save")]
                    async fn js_save(&self, value: u32) -> u32;
                }
            },
        )?;

        assert!(
            output.contains("js_trait async methods must return Result"),
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
                    #[wasm_bindgen(js_name = "put")]
                    fn js_put(&mut self, value: u32);
                }
            },
        )?;

        assert!(
            output.contains("must use `&self` or no receiver"),
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
                    #[wasm_bindgen(js_name = "put")]
                    fn js_put(self, value: u32);
                }
            },
        )?;

        assert!(
            output.contains("must use `&self` or no receiver"),
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
            output.contains("cannot have generic parameters"),
            "generic methods must produce a compile error.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn raw_identifiers_are_unrawed_in_ts_and_tags() -> TestResult {
        let output = expand(
            quote!(js_type = JsKind),
            quote! {
                pub trait r#Kind {
                    #[wasm_bindgen(js_name = "get")]
                    fn js_get(&self, r#type: String) -> String;
                }
            },
        )?;

        assert!(
            output.contains("export interface Kind {"),
            "TS interface name must not contain `r#`.\nOutput: {output}",
        );
        assert!(
            output.contains("get(type: string): string;"),
            "TS parameter names must not contain `r#`.\nOutput: {output}",
        );
        assert!(
            output.contains("__WASM_TRAIT_TS_KIND") && output.contains(r#"("js_get" , "get" ,"#),
            "generated idents and conformance rows must not contain `r#`.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn ts_reserved_param_names_are_escaped() -> TestResult {
        let output = expand(
            quote!(js_type = JsLoop),
            quote! {
                pub trait Loop {
                    #[wasm_bindgen(js_name = "run")]
                    fn js_run(&self, r#for: u32, new: u32) -> u32;
                }
            },
        )?;

        assert!(
            output.contains("run(for_: number, new_: number): number;"),
            "reserved TS words must not be emitted as parameter names.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn bare_identifier_js_name_is_accepted() -> TestResult {
        let output = expand(
            quote!(js_type = JsCounter),
            quote! {
                pub trait Counter {
                    #[wasm_bindgen(js_name = getCount)]
                    fn get_count(&self) -> u32;
                }
            },
        )?;

        assert!(
            output.contains(r#"("get_count" , "getCount" ,"#),
            "`js_name = getCount` must be read like `js_name = \"getCount\"`.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn error_on_default_method_body() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "greet")]
                    fn js_greet(&self) -> String { String::from("hi") }
                }
            },
        )?;

        assert!(
            output.contains("cannot have default bodies"),
            "default method bodies must produce a compile error.\nOutput: {output}",
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
            output.contains("cannot have where clauses"),
            "method where clauses must produce a compile error.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn error_on_catch_with_async() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(catch, js_name = "fetch")]
                    async fn js_fetch(&self) -> Result<JsValue, JsValue>;
                }
            },
        )?;

        assert!(
            output.contains("async methods cannot use `catch`"),
            "catch on async must produce a compile error.\nOutput: {output}",
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
            output.contains("arguments must be simple identifiers"),
            "destructuring patterns must produce a compile error.\nOutput: {output}",
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
            extern_section.contains(r#"wasm_bindgen (method , catch , js_name = "tryLoad")"#),
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

    // Full expansion inspection tests

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
            !tr.contains("# [wasm_bindgen"),
            "trait must not contain wasm_bindgen attrs.\nTrait: {tr}",
        );
        assert!(
            tr.contains(r#"("get_count" , "getCount" , "`Counter::get_count` must be exported to JS as \"getCount\""#),
            "trait must carry a per-method JS name row with a message.\nTrait: {tr}",
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
            !ts.contains("    create("),
            "a static method is not a member of the interface.\nTS: {ts}",
        );
        assert!(
            ts.contains(r"declare global {\n    var create: (label: string) => boolean;\n}"),
            "a static method is declared as the global it binds to.\nTS: {ts}",
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

    // TS type mapping stress tests (QA round 2)

    // Deeply nested generic types

    // Parse error recovery

    #[test]
    fn error_on_empty_args() {
        let result = expand(
            quote!(),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "bar")]
                    fn js_bar(&self);
                }
            },
        );

        let err = result.expect_err("empty #[js_trait()] args must produce an error");
        assert!(err
            .to_string()
            .contains("js_trait requires `js_type = Identifier`"));
    }

    #[test]
    fn error_on_string_js_type() {
        let result = expand(
            quote!(js_type = "NotAnIdent"),
            quote! {
                pub trait Foo {
                    #[wasm_bindgen(js_name = "bar")]
                    fn js_bar(&self);
                }
            },
        );

        let err = result.expect_err("string literal for js_type must produce an error");
        assert!(err
            .to_string()
            .contains("js_type must be a simple identifier"));
    }

    // Trait with supertraits

    #[test]
    fn trait_with_supertrait_compiles() -> TestResult {
        let output = expand(
            quote!(js_type = JsDerived),
            quote! {
                pub trait Derived: Send + Sync {
                    #[wasm_bindgen(js_name = "value")]
                    fn js_value(&self) -> u32;
                }
            },
        )?;

        assert!(
            output.contains("pub trait Derived : Send + Sync {"),
            "supertraits must be preserved.\nOutput: {output}",
        );
        Ok(())
    }

    #[test]
    fn error_on_associated_type() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    type Item;
                }
            },
        )?;

        assert!(output.contains("js_trait does not support associated types"));
        Ok(())
    }

    #[test]
    fn error_on_associated_const() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    const N: u32;
                }
            },
        )?;

        assert!(output.contains("js_trait does not support associated constants"));
        Ok(())
    }

    #[test]
    fn doc_comments_are_kept_on_trait_methods() -> TestResult {
        let output = expand(
            quote!(js_type = JsFoo),
            quote! {
                pub trait Foo {
                    /// Sends the bytes.
                    #[wasm_bindgen(js_name = "send")]
                    fn js_send(&self);
                }
            },
        )?;

        let trait_section = output
            .split_once("pub trait Foo")
            .ok_or("must have trait")?
            .1;
        assert!(
            trait_section.contains(r#"# [doc = r" Sends the bytes."] fn js_send (& self) ;"#),
            "doc comments must survive on the trait method.\nTrait: {trait_section}",
        );
        Ok(())
    }

    fn parse_args(tokens: proc_macro2::TokenStream) -> syn::Result<JsTraitArgs> {
        syn::parse2(tokens)
    }

    #[test]
    fn args_error_on_unknown_key() {
        let err = parse_args(quote!(js_type = JsFoo, typo = Bar))
            .err()
            .expect("unknown key must be rejected");
        assert!(err.to_string().contains("unknown attribute `typo`"));
    }

    #[test]
    fn args_error_on_path_js_type() {
        let err = parse_args(quote!(js_type = a::JsFoo))
            .err()
            .expect("a multi-segment path must be rejected");
        assert!(err
            .to_string()
            .contains("js_type must be a simple identifier"));
    }

    #[test]
    fn args_error_on_non_string_literal_js_name() {
        let err = parse_args(quote!(js_type = JsFoo, js_name = 42))
            .err()
            .expect("a numeric js_name must be rejected");
        assert!(err
            .to_string()
            .contains("js_name must be an identifier or string literal"));
    }

    /// `js_type`/`js_name` parse to the same values in any order, with or
    /// without a trailing comma, and with `js_name` as an ident or a string.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn args_parse_any_order_and_form() {
        bolero::check!()
            .with_type::<(String, String, bool, bool, bool)>()
            .for_each(|(ty, name, swap, trailing, as_str)| {
                let (Ok(ty), Ok(name)) =
                    (syn::parse_str::<Ident>(ty), syn::parse_str::<Ident>(name))
                else {
                    return;
                };
                let name_str = name.to_string();
                let name_tokens = if *as_str {
                    quote!(#name_str)
                } else {
                    quote!(#name)
                };
                let pairs = if *swap {
                    quote!(js_name = #name_tokens, js_type = #ty)
                } else {
                    quote!(js_type = #ty, js_name = #name_tokens)
                };
                let tokens = if *trailing { quote!(#pairs,) } else { pairs };

                let args = parse_args(tokens).expect("valid args must parse");
                assert_eq!(args.js_type, ty);
                assert_eq!(args.js_name.as_deref(), Some(name_str.as_str()));
            });
    }

    #[test]
    fn colliding_escaped_param_names_are_deduplicated() -> TestResult {
        let output = expand(
            quote!(js_type = JsPair),
            quote! {
                pub trait Pair {
                    #[wasm_bindgen(js_name = "collide")]
                    fn js_collide(&self, new: u32, new_: u32) -> u32;
                }
            },
        )?;

        assert!(
            output.contains("collide(new_: number, new__: number): number;"),
            "Output: {output}",
        );
        Ok(())
    }

    #[test]
    fn non_identifier_method_name_is_quoted() -> TestResult {
        let output = expand(
            quote!(js_type = JsOdd),
            quote! {
                pub trait Odd {
                    #[wasm_bindgen(js_name = "my-method")]
                    fn js_my_method(&self) -> u32;
                }
            },
        )?;

        assert!(
            output.contains(r#"\"my-method\"(): number;"#),
            "Output: {output}"
        );
        Ok(())
    }

    #[test]
    fn error_on_non_identifier_interface_name() {
        let err = parse_args(quote!(js_type = JsOdd, js_name = "Odd-Interface"))
            .err()
            .expect("a non-identifier interface name must be rejected");
        assert!(err
            .to_string()
            .contains("not a valid TypeScript interface name"));
    }

    #[test]
    fn error_on_reserved_interface_name() {
        let err = parse_args(quote!(js_type = JsOdd, js_name = "string"))
            .err()
            .expect("a builtin type name must be rejected");
        assert!(err.to_string().contains("reserved in TypeScript"));
    }

    #[test]
    fn error_on_reserved_default_interface_name() -> TestResult {
        let output = expand(
            quote!(js_type = JsAny),
            quote! {
                pub trait r#any {
                    #[wasm_bindgen(js_name = "x")]
                    fn js_x(&self);
                }
            },
        )?;

        assert!(
            output.contains("reserved in TypeScript"),
            "Output: {output}"
        );
        Ok(())
    }

    #[test]
    fn static_method_in_namespace_is_declared_there() -> TestResult {
        let output = expand(
            quote!(js_type = JsFactory),
            quote! {
                pub trait Factory {
                    #[wasm_bindgen(js_namespace = ["app", "factory"], js_name = "make")]
                    fn js_make(name: String) -> u32;
                }
            },
        )?;

        assert!(
            output.contains(
                r"declare global {\n    namespace app.factory {\n        var make: (name: string) => number;\n    }\n}"
            ),
            "Output: {output}",
        );
        Ok(())
    }

    #[test]
    fn static_method_from_module_is_not_declared() -> TestResult {
        let output = expand(
            quote!(js_type = JsFactory),
            quote! {
                pub trait Factory {
                    #[wasm_bindgen(module = "/factory.js", js_name = "make")]
                    fn js_make(name: String) -> u32;
                }
            },
        )?;

        assert!(!output.contains("declare global"), "Output: {output}");
        assert!(!output.contains("make("), "Output: {output}");
        Ok(())
    }

    #[test]
    fn error_on_static_method_with_non_identifier_js_name() -> TestResult {
        let output = expand(
            quote!(js_type = JsFactory),
            quote! {
                pub trait Factory {
                    #[wasm_bindgen(js_name = "make-it")]
                    fn js_make(name: String) -> u32;
                }
            },
        )?;

        assert!(
            output.contains("must be a non-reserved JS identifier"),
            "Output: {output}"
        );
        Ok(())
    }
}
