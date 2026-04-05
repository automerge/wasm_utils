//! Utilities for parsing and filtering `#[wasm_bindgen(...)]` attributes.

use alloc::vec::Vec;
use syn::Attribute;

/// Check whether an attribute is `#[wasm_bindgen(...)]`.
pub fn is_wasm_bindgen(attr: &Attribute) -> bool {
    attr.path().is_ident("wasm_bindgen")
}

/// Partition attributes into wasm_bindgen attrs and everything else.
pub fn partition_attrs(attrs: &[Attribute]) -> (Vec<&Attribute>, Vec<&Attribute>) {
    let mut wb = Vec::new();
    let mut other = Vec::new();

    for attr in attrs {
        if is_wasm_bindgen(attr) {
            wb.push(attr);
        } else {
            other.push(attr);
        }
    }

    (wb, other)
}

/// Strip all `#[wasm_bindgen(...)]` attributes from a list, returning only non-wasm_bindgen attrs.
pub fn strip_wasm_bindgen(attrs: &[Attribute]) -> Vec<Attribute> {
    attrs
        .iter()
        .filter(|a| !is_wasm_bindgen(a))
        .cloned()
        .collect()
}
