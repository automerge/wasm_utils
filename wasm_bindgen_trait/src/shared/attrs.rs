//! Utilities for parsing and filtering `#[wasm_bindgen(...)]` attributes.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
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

/// Extract the `js_name` value from `#[wasm_bindgen(js_name = ...)]` attributes.
///
/// Accepts both forms wasm-bindgen accepts: a string literal
/// (`js_name = "getCount"`) and a bare identifier (`js_name = getCount`).
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
                        if let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(s),
                            ..
                        }) = &nv.value
                        {
                            return Some(s.value());
                        }
                        if let syn::Expr::Path(path) = &nv.value {
                            if let Some(ident) = path.path.get_ident() {
                                return Some(ident.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use quote::quote;
    use syn::{parse_quote, Ident};

    #[test]
    fn strip_wasm_bindgen_keeps_other_attrs() {
        let attrs: Vec<Attribute> = vec![
            parse_quote!(#[doc = "kept"]),
            parse_quote!(#[wasm_bindgen(js_name = "x")]),
            parse_quote!(#[allow(dead_code)]),
        ];
        let kept = strip_wasm_bindgen(&attrs);
        let kept = quote!(#(#kept)*).to_string();
        assert_eq!(
            kept,
            quote!(#[doc = "kept"] #[allow(dead_code)]).to_string()
        );
    }

    #[test]
    fn extract_js_name_absent() {
        let attrs: Vec<Attribute> = vec![
            parse_quote!(#[doc = "js_name = \"nope\""]),
            parse_quote!(#[wasm_bindgen(method, catch)]),
        ];
        assert_eq!(extract_js_name_from_attrs(&attrs), None);
    }

    /// `js_name` is found as a string or a bare ident, wherever it sits among
    /// other `wasm_bindgen` metas and other attributes.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn extract_js_name_finds_any_form_and_position() {
        bolero::check!().with_type::<(String, bool, u8)>().for_each(
            |(name, as_ident, position)| {
                let js_name = if *as_ident {
                    let Ok(ident) = syn::parse_str::<Ident>(name) else {
                        return;
                    };
                    quote!(js_name = #ident)
                } else {
                    quote!(js_name = #name)
                };
                let metas = match position % 3 {
                    0 => quote!(#js_name),
                    1 => quote!(method, #js_name),
                    _ => quote!(method, #js_name, catch),
                };
                let attrs: Vec<Attribute> = vec![
                    parse_quote!(#[doc = "docs"]),
                    parse_quote!(#[wasm_bindgen(#metas)]),
                ];

                assert_eq!(
                    extract_js_name_from_attrs(&attrs).as_deref(),
                    Some(name.as_str())
                );
            },
        );
    }
}
