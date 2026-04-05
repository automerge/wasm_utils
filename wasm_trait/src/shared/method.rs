//! Utilities for manipulating method signatures.
//!
//! Handles stripping `async`, `pub`, and transforming
//! `async fn foo() -> T` into `fn foo() -> impl Future<Output = T> + '_`.

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, ImplItemFn, Pat, ReturnType, Signature};

/// Extract argument names from a method signature, skipping `self`.
pub fn arg_names(sig: &Signature) -> Vec<TokenStream> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Receiver(_) => None,
            FnArg::Typed(pat_type) => {
                let pat = &pat_type.pat;
                Some(quote!(#pat))
            }
        })
        .collect()
}

/// Extract argument patterns with types from a method signature, skipping `self`.
pub fn arg_pats_and_types(sig: &Signature) -> Vec<&FnArg> {
    sig.inputs
        .iter()
        .filter(|arg| !matches!(arg, FnArg::Receiver(_)))
        .collect()
}

/// Whether the method has a `&self` receiver.
pub fn has_self_receiver(sig: &Signature) -> bool {
    sig.inputs
        .iter()
        .any(|arg| matches!(arg, FnArg::Receiver(_)))
}

/// Transform an async method signature into RPITIT form.
///
/// `async fn foo(&self, x: T) -> R` becomes
/// `fn foo(&self, x: T) -> impl ::core::future::Future<Output = R> + '_`
///
/// Non-async signatures are returned unchanged.
pub fn to_rpitit_signature(sig: &Signature) -> Signature {
    if !sig.asyncness.is_some() {
        return sig.clone();
    }

    let mut sig = sig.clone();
    sig.asyncness = None;

    let output_ty = match &sig.output {
        ReturnType::Default => quote!(()),
        ReturnType::Type(_, ty) => quote!(#ty),
    };

    sig.output = ReturnType::Type(
        syn::token::RArrow::default(),
        Box::new(syn::parse_quote! {
            impl ::core::future::Future<Output = #output_ty> + '_
        }),
    );

    sig
}

/// Whether a method is `pub`.
pub fn is_pub(method: &ImplItemFn) -> bool {
    matches!(method.vis, syn::Visibility::Public(_))
}

/// Extract the method name as a string from a pattern.
pub fn pat_ident_name(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(pat_ident) => Some(pat_ident.ident.to_string()),
        _ => None,
    }
}
