//! Utilities for manipulating method signatures.
//!
//! Handles stripping `async`, `pub`, and transforming
//! `async fn foo() -> T` into `fn foo() -> impl Future<Output = T>`.

use alloc::{boxed::Box, vec::Vec};
use quote::{format_ident, quote};
use syn::{FnArg, Ident, ImplItemFn, ReturnType, Signature};

/// Rename every non-`self` parameter to a fresh identifier (`__arg0`, ...).
///
/// Generated code forwards parameters as call arguments, which only works
/// for plain identifiers. A user's pattern may be `mut v`, `_`, or `ref x`;
/// rewriting the signature gives each parameter a name that can be passed on.
/// Returns the rewritten signature and the names, in order.
pub(crate) fn with_fresh_args(sig: &Signature) -> (Signature, Vec<Ident>) {
    let mut sig = sig.clone();
    let mut names = Vec::new();
    for arg in &mut sig.inputs {
        if let FnArg::Typed(pat_type) = arg {
            let name = format_ident!("__arg{}", names.len());
            *pat_type.pat = syn::parse_quote!(#name);
            names.push(name);
        }
    }
    (sig, names)
}

/// Whether the method has any receiver (`self`, `&self`, `&mut self`).
pub(crate) fn has_self_receiver(sig: &Signature) -> bool {
    sig.inputs
        .iter()
        .any(|arg| matches!(arg, FnArg::Receiver(_)))
}

/// Transform an async method signature into RPITIT form.
///
/// `async fn foo(&self, x: T) -> R` becomes
/// `fn foo(&self, x: T) -> impl ::core::future::Future<Output = R>`
///
/// No `+ '_` bound: return-position `impl Trait` in a trait impl already
/// captures every lifetime in scope, including borrowed parameters like
/// `key: &str`. A `'_` bound would tie the future to `&self` alone and reject
/// bodies that capture other borrows.
///
/// Non-async signatures are returned unchanged.
pub(crate) fn to_rpitit_signature(sig: &Signature) -> Signature {
    if sig.asyncness.is_none() {
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
            impl ::core::future::Future<Output = #output_ty>
        }),
    );

    sig
}

/// Whether a method is `pub`.
pub(crate) const fn is_pub(method: &ImplItemFn) -> bool {
    matches!(method.vis, syn::Visibility::Public(_))
}
