//! Names as they appear in the generated TypeScript declarations.
//!
//! Rust identifiers and `js_name` strings don't always make valid TS: Rust
//! allows `new` or `r#for` as parameter names, and wasm-bindgen accepts any
//! string as a `js_name`. These helpers turn them into TS that `tsc` accepts,
//! or report why that's impossible.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use syn::{ext::IdentExt, Ident};

/// Words that can't be a binding name (parameter, `var`, namespace) in a
/// TypeScript declaration file.
///
/// ECMAScript reserved words, strict-mode reserved words (`.d.ts` modules are
/// strict), and `arguments`/`eval`, which strict mode forbids as bindings.
/// Kept sorted for binary search.
const RESERVED_BINDING_NAMES: &[&str] = &[
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

/// Builtin TS type names that can't name an interface. Sorted.
const RESERVED_TYPE_NAMES: &[&str] = &[
    "any",
    "bigint",
    "boolean",
    "never",
    "number",
    "object",
    "string",
    "symbol",
    "undefined",
    "unknown",
];

fn is_reserved_binding(name: &str) -> bool {
    RESERVED_BINDING_NAMES.binary_search(&name).is_ok()
}

/// Whether `name` is a JavaScript identifier (ignoring reserved words).
///
/// Uses Unicode `XID_Start`/`XID_Continue` (what Rust identifiers use) plus
/// `$`. Every XID character is also valid in a JS identifier, so any Rust
/// identifier passes; the few JS-only (`ID_*` but not `XID_*`) characters are
/// conservatively rejected, which only means such names get quoted.
pub(crate) fn is_js_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| unicode_ident::is_xid_start(c) || c == '_' || c == '$')
        && chars.all(|c| unicode_ident::is_xid_continue(c) || c == '$')
}

/// The TS parameter names for a method's arguments, in order.
///
/// Drops any `r#` prefix, appends `_` to reserved words (`r#for` → `for_`,
/// `new` → `new_`), and keeps appending `_` until each name is unique within
/// the method (`new, new_` → `new_, new__`). Parameter names in a `.d.ts` are
/// documentation only, so renaming them is safe.
pub(crate) fn ts_param_names<'a, I: IntoIterator<Item = &'a Ident>>(idents: I) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for ident in idents {
        let mut name = ident.unraw().to_string();
        if is_reserved_binding(&name) {
            name.push('_');
        }
        while names.contains(&name) {
            name.push('_');
        }
        names.push(name);
    }
    names
}

/// A method name as a TS interface member: bare if it's an identifier,
/// otherwise a quoted string literal (`"my-method"`).
///
/// Reserved words are fine as member names, so they stay bare.
pub(crate) fn ts_member_name(js_name: &str) -> String {
    if is_js_identifier(js_name) {
        js_name.to_string()
    } else {
        quote_js_string(js_name)
    }
}

/// Why `name` can't be a TS interface name, if it can't.
pub(crate) fn interface_name_error(name: &str) -> Option<String> {
    if !is_js_identifier(name) {
        Some(format!(
            "`{name}` is not a valid TypeScript interface name (it must be a JS identifier)"
        ))
    } else if is_reserved_binding(name) || RESERVED_TYPE_NAMES.binary_search(&name).is_ok() {
        Some(format!(
            "`{name}` is reserved in TypeScript and can't name an interface"
        ))
    } else {
        None
    }
}

/// Why `name` can't be the global binding a static method is declared as.
pub(crate) fn binding_name_error(name: &str) -> Option<String> {
    if !is_js_identifier(name) || is_reserved_binding(name) {
        Some(format!(
            "static js_trait methods bind to a JS global, so the JS name `{name}` must be \
             a non-reserved JS identifier"
        ))
    } else {
        None
    }
}

/// A double-quoted JS string literal for `s`.
fn quote_js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            // `\u{7f}` is also JS (ES2015) escape syntax.
            c if c.is_control() => out.extend(c.escape_unicode()),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn idents(names: &[&str]) -> Vec<Ident> {
        names
            .iter()
            .map(|n| syn::parse_str::<Ident>(n).expect("valid ident"))
            .collect()
    }

    #[test]
    fn reserved_lists_are_sorted_and_unique() {
        for list in [RESERVED_BINDING_NAMES, RESERVED_TYPE_NAMES] {
            assert!(list.windows(2).all(|pair| matches!(pair, [a, b] if a < b)));
        }
    }

    #[test]
    fn param_names_escape_unraw_and_dedupe() {
        assert_eq!(
            ts_param_names(&idents(&["r#for", "r#type", "new", "new_", "value"])),
            vec!["for_", "type", "new_", "new__", "value"]
        );
    }

    #[test]
    fn member_names_quote_only_non_identifiers() {
        assert_eq!(ts_member_name("getCount"), "getCount");
        assert_eq!(ts_member_name("delete"), "delete");
        assert_eq!(ts_member_name("my-method"), r#""my-method""#);
        assert_eq!(ts_member_name(r#"a"b\c"#), r#""a\"b\\c""#);
    }

    #[test]
    fn interface_names() {
        assert_eq!(interface_name_error("Widget"), None);
        assert!(interface_name_error("Odd-Interface").is_some());
        assert!(interface_name_error("class").is_some());
        assert!(interface_name_error("string").is_some());
    }

    /// For any argument list, the TS names are pairwise distinct, never
    /// reserved, and start from the unraw'd Rust name.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn param_names_are_unique_and_never_reserved() {
        bolero::check!()
            .with_type::<Vec<(String, bool)>>()
            .for_each(|raw| {
                // Bias toward collisions: many args share a few names, with or
                // without a trailing `_`.
                let args: Vec<Ident> = raw
                    .iter()
                    .filter_map(|(name, underscore)| {
                        let bases = ["new", "for", "value", "x"];
                        let pick = name.len() % bases.len();
                        let base = bases.get(pick).copied().unwrap_or("new");
                        let name = if *underscore {
                            format!("{base}_")
                        } else {
                            base.to_string()
                        };
                        syn::parse_str::<Ident>(&name).ok()
                    })
                    .collect();

                let names = ts_param_names(&args);
                assert_eq!(names.len(), args.len());
                for (i, (name, ident)) in names.iter().zip(&args).enumerate() {
                    assert!(!is_reserved_binding(name), "{name} is reserved");
                    assert!(name.starts_with(&ident.unraw().to_string()));
                    assert!(names.iter().skip(i + 1).all(|other| other != name));
                }
            });
    }

    /// A member name is the identifier itself, or a quoted literal that
    /// decodes back to the original string.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn member_names_round_trip() {
        fn unquote(s: &str) -> String {
            let inner = s
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .expect("quoted");
            let mut out = String::new();
            let mut chars = inner.chars();
            while let Some(c) = chars.next() {
                if c != '\\' {
                    out.push(c);
                    continue;
                }
                match chars.next().expect("no dangling escape") {
                    'u' => {
                        let hex: String =
                            chars.by_ref().skip(1).take_while(|&c| c != '}').collect();
                        let code = u32::from_str_radix(&hex, 16).expect("hex escape");
                        out.push(char::from_u32(code).expect("valid scalar"));
                    }
                    other => out.push(other),
                }
            }
            out
        }

        bolero::check!().with_type::<String>().for_each(|name| {
            let member = ts_member_name(name);
            if is_js_identifier(name) {
                assert_eq!(&member, name);
            } else {
                assert_eq!(&unquote(&member), name);
            }
        });
    }

    /// Every Rust identifier, unraw'd, is a JS identifier.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn rust_identifiers_are_js_identifiers() {
        bolero::check!().with_type::<String>().for_each(|s| {
            if let Ok(ident) = syn::parse_str::<Ident>(s) {
                assert!(is_js_identifier(&ident.unraw().to_string()), "{ident}");
            }
        });
    }
}
