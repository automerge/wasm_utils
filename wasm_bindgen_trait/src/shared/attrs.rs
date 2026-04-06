//! Utilities for parsing and filtering `#[wasm_bindgen(...)]` attributes.

use alloc::{string::String, vec::Vec};
use syn::{punctuated::Punctuated, Attribute, Token};

/// Check whether an attribute is `#[wasm_bindgen(...)]`.
pub(crate) fn is_wasm_bindgen(attr: &Attribute) -> bool {
    attr.path().is_ident("wasm_bindgen")
}

/// Strip all `#[wasm_bindgen(...)]` attributes from a list, returning only non-wasm_bindgen attrs.
pub(crate) fn strip_wasm_bindgen(attrs: &[Attribute]) -> Vec<Attribute> {
    attrs
        .iter()
        .filter(|a| !is_wasm_bindgen(a))
        .cloned()
        .collect()
}

/// Extract the `js_name` value from `#[wasm_bindgen(js_name = "...")]` attributes.
///
/// Returns `None` if no `js_name` is found.
pub(crate) fn extract_js_name_from_attrs(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if !is_wasm_bindgen(attr) {
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
