//! Naming conventions for generated code.

use alloc::{
    format,
    string::{String, ToString},
};
use heck::ToSnakeCase;
use proc_macro2::Span;
use syn::{ext::IdentExt, Ident};

/// Generate the Rust ident for the runtime tag method.
///
/// Given trait name `Transport`, returns `__wasm_impl_transport`.
pub(crate) fn tag_method_rust_ident(trait_name: &Ident) -> Ident {
    let snake = trait_name.unraw().to_string().to_snake_case();
    Ident::new(&format!("__wasm_impl_{snake}"), Span::call_site())
}

/// Generate the JS name string for the runtime tag method.
///
/// Given trait name `Transport`, returns `"__wasm_impl_Transport"`.
pub(crate) fn tag_method_js_name(trait_name: &Ident) -> String {
    format!("__wasm_impl_{}", trait_name.unraw())
}

/// Generate the Rust ident for a `__wasm_trait_`-prefixed extern fn.
///
/// Given method name `js_send_bytes`, returns `__wasm_trait_js_send_bytes`.
pub(crate) fn extern_fn_ident(method_name: &Ident) -> Ident {
    Ident::new(
        &format!("__wasm_trait_{}", method_name.unraw()),
        method_name.span(),
    )
}

/// Words that can't be a parameter name in a TypeScript declaration file.
///
/// ECMAScript reserved words, strict-mode reserved words (`.d.ts` modules are
/// strict), and `arguments`/`eval`, which strict mode forbids as bindings.
/// Kept sorted for binary search.
const TS_RESERVED_PARAM_NAMES: &[&str] = &[
    "arguments",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "eval",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// The TypeScript parameter name for a Rust argument identifier.
///
/// Drops any `r#` prefix and appends `_` to names that are reserved in
/// TypeScript, so `r#for` → `for_` and `new` → `new_`. Parameter names in a
/// `.d.ts` are documentation only, so renaming them is safe.
pub(crate) fn ts_param_name(ident: &Ident) -> String {
    let name = ident.unraw().to_string();
    if TS_RESERVED_PARAM_NAMES
        .binary_search(&name.as_str())
        .is_ok()
    {
        format!("{name}_")
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_list_is_sorted_and_unique() {
        assert!(TS_RESERVED_PARAM_NAMES
            .windows(2)
            .all(|pair| matches!(pair, [a, b] if a < b)));
    }

    #[test]
    fn ts_param_name_escapes_reserved_and_unraws() {
        let name = |s: &str| ts_param_name(&syn::parse_str::<Ident>(s).expect("valid ident"));
        assert_eq!(name("r#for"), "for_");
        assert_eq!(name("r#type"), "type");
        assert_eq!(name("new"), "new_");
        assert_eq!(name("value"), "value");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn ts_param_name_is_never_reserved() {
        bolero::check!()
            .with_type::<String>()
            .filter(|s| syn::parse_str::<Ident>(s).is_ok())
            .for_each(|s| {
                let ident = syn::parse_str::<Ident>(s).expect("filtered to valid idents");
                let out = ts_param_name(&ident);
                let unraw = ident.unraw().to_string();

                assert!(TS_RESERVED_PARAM_NAMES
                    .binary_search(&out.as_str())
                    .is_err());
                if TS_RESERVED_PARAM_NAMES
                    .binary_search(&unraw.as_str())
                    .is_ok()
                {
                    assert_eq!(out, format!("{unraw}_"));
                } else {
                    assert_eq!(out, unraw);
                }
            });
    }
}
