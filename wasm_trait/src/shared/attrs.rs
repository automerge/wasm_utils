//! Utilities for parsing and filtering `#[wasm_bindgen(...)]` attributes.

use alloc::vec::Vec;
use syn::Attribute;

/// Check whether an attribute is `#[wasm_bindgen(...)]`.
pub fn is_wasm_bindgen(attr: &Attribute) -> bool {
    attr.path().is_ident("wasm_bindgen")
}

/// Strip all `#[wasm_bindgen(...)]` attributes from a list, returning only non-wasm_bindgen attrs.
pub fn strip_wasm_bindgen(attrs: &[Attribute]) -> Vec<Attribute> {
    attrs
        .iter()
        .filter(|a| !is_wasm_bindgen(a))
        .cloned()
        .collect()
}
