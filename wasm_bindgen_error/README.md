# wasm_bindgen_error

Derive macro for converting Rust error types into named JS errors via `wasm-bindgen`.

## Usage

```rust
use wasm_bindgen_error::WasmError;

// JS error name defaults to "HydrationError" (strips "Wasm" prefix)
#[derive(Debug, WasmError)]
#[error(transparent)]
pub struct WasmHydrationError(#[from] HydrationError);

// Explicit JS name override
#[derive(Debug, WasmError)]
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
2. Otherwise, strip a leading `Wasm` prefix from the Rust type name.

| Rust Type            | JS Error Name    |
|----------------------|------------------|
| `WasmHydrationError` | `HydrationError` |
| `WasmConnectError`   | `ConnectError`   |
| `JsStorageError`     | `JsStorageError` |

## Requirements

The type must implement `core::error::Error` (e.g. via `thiserror`, a manual impl, etc.).

## License

Apache-2.0
