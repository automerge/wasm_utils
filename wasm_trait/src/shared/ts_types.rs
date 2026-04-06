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

                // JS sys types
                "Uint8Array" => "Uint8Array".into(),
                "Uint16Array" => "Uint16Array".into(),
                "Uint32Array" => "Uint32Array".into(),
                "Int8Array" => "Int8Array".into(),
                "Int16Array" => "Int16Array".into(),
                "Int32Array" => "Int32Array".into(),
                "Float32Array" => "Float32Array".into(),
                "Float64Array" => "Float64Array".into(),
                "Array" => "Array<any>".into(),
                "ArrayBuffer" => "ArrayBuffer".into(),
                "Function" => "Function".into(),
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
                            if inner_ts == "number" {
                                // Numeric vectors currently map to Array<number>.
                                return "Array<number>".into();
                            }
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
