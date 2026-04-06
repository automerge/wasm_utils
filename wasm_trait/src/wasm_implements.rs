//! Implementation of the `#[wasm_implements(TraitName)]` attribute macro.
//!
//! Placed on a `#[wasm_bindgen]` impl block, generates:
//! 1. A runtime tag method (`__wasm_impl_{TraitName}`) injected into the impl block
//! 2. A hidden `const _: () = { impl Trait for Type { ... } }` witness
//!    for compile-time signature checking

use alloc::vec::Vec;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{spanned::Spanned, ImplItem, ImplItemFn, ItemImpl, Path};

use crate::shared::{
    attrs::strip_wasm_bindgen,
    method::{arg_names, has_self_receiver, is_pub, to_rpitit_signature},
    naming::{tag_method_js_name, tag_method_rust_ident},
};

/// Core implementation of `#[wasm_implements]`.
pub(crate) fn wasm_implements_impl(trait_path: &Path, mut impl_block: ItemImpl) -> TokenStream {
    // Reject trait impls — we need an inherent impl
    if let Some((_, path, _)) = &impl_block.trait_ {
        return syn::Error::new(
            path.span(),
            "#[wasm_implements] must be used on an inherent impl, not a trait impl",
        )
        .to_compile_error();
    }

    let self_ty = &impl_block.self_ty;

    // Extract the last segment of the trait path for naming
    let trait_name = match trait_path.segments.last() {
        Some(seg) => &seg.ident,
        None => {
            return syn::Error::new(
                trait_path.span(),
                "#[wasm_implements] requires a trait path",
            )
            .to_compile_error();
        }
    };

    // Collect all pub fn items from the impl block
    let pub_methods: Vec<&ImplItemFn> = impl_block
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Fn(method) if is_pub(method) => Some(method),
            _ => None,
        })
        .collect();

    // Generate the witness methods
    let witness_methods: Vec<TokenStream> = pub_methods
        .iter()
        .map(|method| {
            let sig = &method.sig;
            let method_name = &sig.ident;

            // Transform to RPITIT if async
            let witness_sig = to_rpitit_signature(sig);

            // Strip wasm_bindgen attrs
            let clean_attrs = strip_wasm_bindgen(&method.attrs);

            let arg_names: Vec<_> = arg_names(sig);

            let body: TokenStream = if has_self_receiver(sig) {
                quote! { #self_ty::#method_name(self, #(#arg_names),*) }
            } else {
                quote! { #self_ty::#method_name(#(#arg_names),*) }
            };

            quote! {
                #(#clean_attrs)*
                #witness_sig {
                    #body
                }
            }
        })
        .collect();

    // Generate the runtime tag method
    let tag_rust_name = tag_method_rust_ident(trait_name);
    let tag_js_name = tag_method_js_name(trait_name);

    let tag_method: syn::ImplItemFn = syn::parse_quote! {
        #[wasm_bindgen(js_name = #tag_js_name)]
        pub fn #tag_rust_name(&self) -> bool {
            true
        }
    };

    // Inject tag method into the original impl block
    impl_block.items.push(ImplItem::Fn(tag_method));

    // Generate the hidden witness
    let witness = quote! {
        const _: () = {
            impl #trait_path for #self_ty {
                #(#witness_methods)*
            }
        };
    };

    quote! {
        #impl_block
        #witness
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use quote::quote;

    /// Helper: run the macro transform on the given trait path and impl block,
    /// returning the output as a `String`.
    fn expand(attr: proc_macro2::TokenStream, item: proc_macro2::TokenStream) -> String {
        let trait_path: Path = syn::parse2(attr).expect("failed to parse trait path");
        let impl_block: ItemImpl = syn::parse2(item).expect("failed to parse impl block");
        wasm_implements_impl(&trait_path, impl_block).to_string()
    }

    #[test]
    fn generates_tag_method() {
        let output = expand(
            quote!(Transport),
            quote! {
                impl WasmFoo {
                    pub fn js_put(&self, value: u32) -> u32 { value }
                }
            },
        );

        assert!(
            output.contains("__wasm_impl_transport"),
            "must inject the runtime tag method.\nOutput: {output}",
        );
        assert!(
            output.contains("__wasm_impl_Transport"),
            "must generate the JS-side tag name.\nOutput: {output}",
        );
    }

    #[test]
    fn generates_witness_impl() {
        let output = expand(
            quote!(Transport),
            quote! {
                impl WasmFoo {
                    pub fn js_put(&self, value: u32) -> u32 { value }
                }
            },
        );

        assert!(
            output.contains("impl Transport for WasmFoo"),
            "must generate a witness trait impl.\nOutput: {output}",
        );
        assert!(
            output.contains("const _ : () ="),
            "witness must be inside const _: ().\nOutput: {output}",
        );
    }

    #[test]
    fn witness_delegates_to_inherent_method() {
        let output = expand(
            quote!(Transport),
            quote! {
                impl WasmFoo {
                    pub fn js_put(&self, value: u32) -> u32 { value }
                }
            },
        );

        assert!(
            output.contains("WasmFoo :: js_put (self , value)")
                || output.contains("WasmFoo :: js_put(self, value)")
                || output.contains("WasmFoo::js_put(self, value)"),
            "witness must delegate to the inherent method.\nOutput: {output}",
        );
    }

    #[test]
    fn strips_wasm_bindgen_attrs_from_witness() {
        let output = expand(
            quote!(Transport),
            quote! {
                impl WasmFoo {
                    #[wasm_bindgen(js_name = "put")]
                    pub fn js_put(&self, value: u32) -> u32 { value }
                }
            },
        );

        // The original impl should still have wasm_bindgen
        let witness_start = output.find("const _ : () =").expect("must have witness");
        let witness_section = &output[witness_start..];

        assert!(
            !witness_section.contains("wasm_bindgen"),
            "witness must NOT contain wasm_bindgen attrs.\nWitness: {witness_section}",
        );
    }

    #[test]
    fn async_method_becomes_rpitit_in_witness() {
        let output = expand(
            quote!(Transport),
            quote! {
                impl WasmFoo {
                    pub async fn js_send(&self, data: u32) -> u32 { data }
                }
            },
        );

        let witness_start = output.find("const _ : () =").expect("must have witness");
        let witness_section = &output[witness_start..];

        assert!(
            witness_section.contains("Future"),
            "async methods in witness must use RPITIT (impl Future).\nWitness: {witness_section}",
        );
        assert!(
            !witness_section.contains("async"),
            "witness must NOT contain async keyword.\nWitness: {witness_section}",
        );
    }

    #[test]
    fn preserves_original_impl_block() {
        let output = expand(
            quote!(Transport),
            quote! {
                impl WasmFoo {
                    #[wasm_bindgen(js_name = "put")]
                    pub fn js_put(&self, value: u32) -> u32 { value }
                }
            },
        );

        // The original impl block should appear before the witness
        let impl_pos = output
            .find("impl WasmFoo")
            .expect("must have original impl");
        let witness_pos = output.find("const _ : () =").expect("must have witness");

        assert!(
            impl_pos < witness_pos,
            "original impl must come before the witness.\nOutput: {output}",
        );
    }

    #[test]
    fn static_method_delegates_without_self() {
        let output = expand(
            quote!(Factory),
            quote! {
                impl WasmFoo {
                    pub fn js_create(name: u32) -> u32 { name }
                }
            },
        );

        let witness_start = output.find("const _ : () =").expect("must have witness");
        let witness_section = &output[witness_start..];

        assert!(
            witness_section.contains("WasmFoo :: js_create (name)")
                || witness_section.contains("WasmFoo :: js_create(name)")
                || witness_section.contains("WasmFoo::js_create(name)"),
            "static methods must delegate without self.\nWitness: {witness_section}",
        );
    }

    #[test]
    fn supports_module_path_trait() {
        let output = expand(
            quote!(my_module::Transport),
            quote! {
                impl WasmFoo {
                    pub fn js_put(&self, value: u32) -> u32 { value }
                }
            },
        );

        assert!(
            output.contains("impl my_module :: Transport for WasmFoo")
                || output.contains("impl my_module::Transport for WasmFoo"),
            "must support module-qualified trait paths.\nOutput: {output}",
        );

        // Tag should use the last segment
        assert!(
            output.contains("__wasm_impl_transport"),
            "tag must use the last segment of the trait path.\nOutput: {output}",
        );
    }

    #[test]
    fn error_on_trait_impl() {
        let output = expand(
            quote!(Transport),
            quote! {
                impl SomeTrait for WasmFoo {
                    fn js_put(&self) {}
                }
            },
        );

        assert!(
            output.contains("compile_error"),
            "trait impl must produce a compile error.\nOutput: {output}",
        );
    }

    #[test]
    fn ignores_private_methods() {
        let output = expand(
            quote!(Transport),
            quote! {
                impl WasmFoo {
                    pub fn js_put(&self, value: u32) -> u32 { value }
                    fn private_helper(&self) -> u32 { 42 }
                }
            },
        );

        let witness_start = output.find("const _ : () =").expect("must have witness");
        let witness_section = &output[witness_start..];

        assert!(
            !witness_section.contains("private_helper"),
            "witness must not include private methods.\nWitness: {witness_section}",
        );
        assert!(
            witness_section.contains("js_put"),
            "witness must include public methods.\nWitness: {witness_section}",
        );
    }
}
