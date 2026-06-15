# wasm_utils

Utilities for working with Rust-exported `wasm-bindgen` types in JS environments.

Solves the problem of using Rust types across the Wasm boundary where
`wasm-bindgen` imposes limitations (no generics, consuming ownership,
no references in `Vec`s, no trait impls, stringly-typed errors, etc.).

## Crates

| Crate                                        | Version | Description                                                                                      |
|----------------------------------------------|---------|--------------------------------------------------------------------------------------------------|
| [`wasm_refgen`](./wasm_refgen)               | 0.2.0   | Proc-macro that generates duck-typed JS reference boilerplate for `wasm-bindgen` structs         |
| [`from_js_ref`](./from_js_ref)               | 0.2.0   | Runtime traits (`FromJsRef`, `JsDeref`) for converting between JS reference types and Rust types |
| [`wasm_bindgen_trait`](./wasm_bindgen_trait) | 0.1.0   | JS duck-typed interfaces as Rust traits with compile-time conformance checking                   |
| [`wasm_bindgen_error`](./wasm_bindgen_error) | 0.1.0   | Derive macro to convert Rust error types into named JS errors                                    |

## Quick Start

### Reference types with `wasm_refgen`

Generate a JS reference type so your exported struct can appear in
`Vec`s, generics, and function signatures:

```rust
use wasm_bindgen::prelude::*;
use wasm_refgen::wasm_refgen;

#[derive(Clone)]
#[wasm_bindgen(js_name = "Foo")]
pub struct WasmFoo {
    inner: u32, // must be cheap to clone
}

#[wasm_refgen(js_ref = JsFoo)]
#[wasm_bindgen(js_class = "Foo")]
impl WasmFoo {
    #[wasm_bindgen(constructor)]
    pub fn new(inner: u32) -> Self {
        Self { inner }
    }
}
```

This generates a `JsFoo` type that can be used in function signatures,
`Vec`s, and generics — places where `wasm-bindgen` normally can't accept
exported Rust types directly.

```rust
use from_js_ref::FromJsRef;

// Convert from a typed reference
pub fn from_ref(foo: &JsFoo) -> WasmFoo {
    foo.into()
}

// Convert from a raw JsValue (duck-type validated)
pub fn from_value(value: &JsValue) -> Option<WasmFoo> {
    WasmFoo::try_from_js_value(value)
}

// Typed Vec support
pub fn from_many(foos: Vec<JsFoo>) -> Vec<WasmFoo> {
    foos.iter().map(Into::into).collect()
}
```

See the [`wasm_refgen` README](./wasm_refgen/README.md) for detailed documentation.

### JS interfaces with `wasm_bindgen_trait`

Define a JS duck-typed interface as a Rust trait, then verify your
exported struct conforms at compile time:

```rust
use wasm_bindgen::prelude::*;
use wasm_bindgen_trait::{js_trait, wasm_implements};

// Define the JS interface
#[js_trait(js_type = JsStorage)]
pub trait Storage {
    #[wasm_bindgen(js_name = "save")]
    async fn js_save(&self, key: String, value: JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = "load")]
    async fn js_load(&self, key: String) -> Result<JsValue, JsValue>;
}

// Export a Rust implementation that conforms to the interface
#[wasm_bindgen]
pub struct WasmMemoryStorage { /* ... */ }

#[wasm_implements(Storage)]
#[wasm_bindgen(js_class = "WasmMemoryStorage")]
impl WasmMemoryStorage {
    #[wasm_bindgen(js_name = "save")]
    pub async fn js_save(&self, key: String, value: JsValue) -> Result<(), JsValue> {
        Ok(())
    }

    #[wasm_bindgen(js_name = "load")]
    pub async fn js_load(&self, key: String) -> Result<JsValue, JsValue> {
        Ok(JsValue::UNDEFINED)
    }
}
```

See the [`wasm_bindgen_trait` README](./wasm_bindgen_trait/README.md) for detailed documentation.

### Named JS errors with `wasm_bindgen_error`

Convert Rust error types into JS `Error` objects with meaningful `.name`
properties:

```rust
use thiserror::Error;
use wasm_bindgen_error::WasmError;

#[derive(Debug, Error)]
#[error("document not found: {id}")]
pub struct NotFoundError { id: String }

// JS sees: { name: "NotFoundError", message: "document not found: abc" }
#[derive(Debug, Error, WasmError)]
#[error(transparent)]
pub struct WasmNotFoundError(#[from] NotFoundError);
```

See the [`wasm_bindgen_error` README](./wasm_bindgen_error/README.md) for detailed documentation.

## License

Apache-2.0
