//! Mapping from Rust types to TypeScript types.
//!
//! Used by `#[js_trait]` to generate `typescript_custom_section` interfaces.

use alloc::string::{String, ToString};
use syn::Type;

/// Map a Rust type to its TypeScript representation.
///
/// Falls back to `"any"` for types that can't be automatically mapped.
pub(crate) fn rust_type_to_ts(ty: &Type) -> String {
    match ty {
        Type::Tuple(tuple) if tuple.elems.is_empty() => "void".into(),

        Type::Path(type_path) => {
            let Some(seg) = type_path.path.segments.last() else {
                return "any".into();
            };

            let name = seg.ident.to_string();
            match name.as_str() {
                // Primitives
                "JsValue" => "any".into(),
                "bool" => "boolean".into(),
                "String" | "str" => "string".into(),
                "u8" | "u16" | "u32" | "i8" | "i16" | "i32" | "f32" | "f64" | "usize" | "isize" => {
                    "number".into()
                }
                "u64" | "i64" | "u128" | "i128" => "bigint".into(),

                // js-sys types whose TS name differs from the Rust name. Others
                // (`Uint8Array`, `ArrayBuffer`, `Function`, ...) fall through.
                "Array" => "Array<any>".into(),
                "Map" => "Map<any, any>".into(),
                "Set" => "Set<any>".into(),
                "Promise" => "Promise<any>".into(),
                "Object" => "object".into(),

                // Generic wrappers
                "Option" => {
                    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                        if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                            let inner_ts = rust_type_to_ts(inner);
                            return alloc::format!("{inner_ts} | null");
                        }
                    }
                    "any | null".into()
                }

                "Result" => {
                    // For Result<T, E>, map to T (the success type).
                    // In async context this becomes Promise<T>.
                    // In catch context this is just T.
                    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                        if let Some(syn::GenericArgument::Type(ok_ty)) = args.args.first() {
                            return rust_type_to_ts(ok_ty);
                        }
                    }
                    "any".into()
                }

                "Vec" => {
                    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                        if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                            let inner_ts = rust_type_to_ts(inner);
                            return alloc::format!("Array<{inner_ts}>");
                        }
                    }
                    "Array<any>".into()
                }

                // Fall through: use the Rust type name as-is
                other => other.into(),
            }
        }

        Type::Reference(type_ref) => rust_type_to_ts(&type_ref.elem),

        Type::Slice(slice) => {
            let inner_ts = rust_type_to_ts(&slice.elem);
            alloc::format!("Array<{inner_ts}>")
        }

        Type::Array(_)
        | Type::BareFn(_)
        | Type::Group(_)
        | Type::ImplTrait(_)
        | Type::Infer(_)
        | Type::Macro(_)
        | Type::Never(_)
        | Type::Paren(_)
        | Type::Ptr(_)
        | Type::TraitObject(_)
        | Type::Verbatim(_)
        | _ => "any".into(),
    }
}

/// Map the return type of an async method for TS.
///
/// `Result<T, E>` → `Promise<T_ts>` (rejection represents E).
/// `()` → `Promise<void>`.
pub(crate) fn async_return_to_ts(ty: &Type) -> String {
    let inner = rust_type_to_ts(ty);
    alloc::format!("Promise<{inner}>")
}

/// Map the return type for TS, handling both sync and async cases.
///
/// For sync methods, maps the type directly.
/// For sync methods returning `()`, maps to `void`.
pub(crate) fn sync_return_to_ts(ret: &syn::ReturnType) -> String {
    match ret {
        syn::ReturnType::Default => "void".into(),
        syn::ReturnType::Type(_, ty) => rust_type_to_ts(ty),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{format, string::String, vec::Vec};

    fn ts(rust: &str) -> String {
        rust_type_to_ts(&syn::parse_str::<Type>(rust).expect("valid Rust type"))
    }

    #[test]
    fn leaf_types_map_to_their_ts_names() {
        let table = [
            ("()", "void"),
            ("JsValue", "any"),
            ("bool", "boolean"),
            ("String", "string"),
            ("str", "string"),
            ("u32", "number"),
            ("f64", "number"),
            ("usize", "number"),
            ("u64", "bigint"),
            ("i128", "bigint"),
            ("js_sys::Array", "Array<any>"),
            ("js_sys::Map", "Map<any, any>"),
            ("js_sys::Set", "Set<any>"),
            ("js_sys::Promise", "Promise<any>"),
            ("js_sys::Object", "object"),
            ("js_sys::Uint8Array", "Uint8Array"),
            ("MyType", "MyType"),
            ("Option", "any | null"),
            ("Result", "any"),
            ("Vec", "Array<any>"),
            ("fn(u32)", "any"),
            ("(u8, u8)", "any"),
        ];
        for (rust, expected) in table {
            assert_eq!(ts(rust), expected, "for `{rust}`");
        }
    }

    #[test]
    fn return_types() {
        let ret = |src: &str| sync_return_to_ts(&syn::parse_str(src).expect("valid return type"));
        assert_eq!(ret(""), "void");
        assert_eq!(ret("-> u32"), "number");
        assert_eq!(
            async_return_to_ts(&syn::parse_str("Result<(), JsValue>").expect("valid type")),
            "Promise<void>"
        );
    }

    const LEAVES: &[&str] = &[
        "()",
        "bool",
        "u8",
        "i64",
        "String",
        "str",
        "JsValue",
        "Uint8Array",
        "MyType",
    ];

    /// Build a Rust type from bytes: the first picks a leaf, each later byte
    /// wraps it in a reference, `Option`, `Vec`, slice, or `Result`.
    fn build_type(bytes: &[u8]) -> String {
        let Some((first, rest)) = bytes.split_first() else {
            return "u8".into();
        };
        let leaf = LEAVES
            .get(usize::from(*first) % LEAVES.len())
            .expect("index is reduced modulo the length");
        let mut ty = String::from(*leaf);
        for byte in rest.iter().take(5) {
            ty = match byte % 5 {
                0 => format!("&{ty}"),
                1 => format!("Option<{ty}>"),
                2 => format!("Vec<{ty}>"),
                3 => format!("&[{ty}]"),
                _ => format!("Result<{ty}, JsValue>"),
            };
        }
        ty
    }

    /// The wrappers map structurally: each one transforms the TS of its
    /// inner type in a fixed way, whatever that inner type is.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn wrappers_map_structurally() {
        bolero::check!().with_type::<Vec<u8>>().for_each(|bytes| {
            let rust = build_type(bytes);
            let inner = ts(&rust);

            assert_eq!(ts(&format!("&{rust}")), inner);
            assert_eq!(ts(&format!("&mut {rust}")), inner);
            assert_eq!(ts(&format!("Option<{rust}>")), format!("{inner} | null"));
            assert_eq!(ts(&format!("Result<{rust}, js_sys::Error>")), inner);
            assert_eq!(ts(&format!("Vec<{rust}>")), format!("Array<{inner}>"));
            assert_eq!(ts(&format!("&[{rust}]")), format!("Array<{inner}>"));
            assert_eq!(
                async_return_to_ts(&syn::parse_str(&rust).expect("valid type")),
                format!("Promise<{inner}>")
            );

            assert!(!inner.is_empty());
            assert_eq!(inner.matches('<').count(), inner.matches('>').count());
        });
    }
}
