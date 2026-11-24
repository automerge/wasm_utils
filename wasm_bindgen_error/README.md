# wasm_bindgen_error

Derive macro for converting Rust error types into named JS errors via `wasm-bindgen`.

## Usage

```rust
use thiserror::Error;
use wasm_bindgen_error::WasmError;

// JS error name defaults to "HydrationError" (strips "Wasm" prefix)
#[derive(Debug, Error, WasmError)]
#[error(transparent)]
pub struct WasmHydrationError(#[from] HydrationError);

// Explicit JS name override
#[derive(Debug, Error, WasmError)]
#[wasm_error(js_name = "StorageFailure")]
#[error(transparent)]
pub struct WasmIoError(#[from] IoError);
```

The derive generates:

```rust
impl From<WasmHydrationError> for JsValue {
    fn from(err: WasmHydrationError) -> Self {
        let js_err = js_sys::Error::new(&err.to_string());
        js_err.set_name("HydrationError");
        js_err.into()
    }
}
```

This gives JS consumers a typed `error.name` instead of a generic `"Error"`.

## JS Error Name Resolution

1. If `#[wasm_error(js_name = "...")]` is present, use that string literally.
2. Otherwise, strip a leading `Wasm` word from the Rust type name. It is only
   stripped at a CamelCase boundary, i.e. when the next character is uppercase.

| Rust Type            | JS Error Name    |
|----------------------|------------------|
| `WasmHydrationError` | `HydrationError` |
| `WasmConnectError`   | `ConnectError`   |
| `JsStorageError`     | `JsStorageError` |
| `WasmtimeError`      | `WasmtimeError`  |

`js_name` may be given at most once, and `#[wasm_error]` is only accepted on the
type itself (not on enum variants or fields).

## Requirements

The type must implement `core::error::Error` (e.g. via `thiserror`, a manual impl, etc.).

The generated code refers to `::wasm_bindgen` and `::js_sys` by absolute path,
so your crate must depend on both directly (without renaming them):

```toml
[dependencies]
js-sys = "0.3"
wasm-bindgen = "0.2"
wasm_bindgen_error = "0.1"
```

## License

Apache-2.0
