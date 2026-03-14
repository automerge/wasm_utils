# wasm_utils

Utilities for working with Rust-exported `wasm-bindgen` types in JS environments.

## Crates

| Crate                          | Version | Description                                                                                      |
|--------------------------------|---------|--------------------------------------------------------------------------------------------------|
| [`wasm_refgen`](./wasm_refgen) | 0.2.0   | Proc-macro that generates duck-typed JS reference boilerplate for `wasm-bindgen` structs         |
| [`from_js_ref`](./from_js_ref) | 0.2.0   | Runtime traits (`FromJsRef`, `JsDeref`) for converting between JS reference types and Rust types |

## Quick Start

```toml
[dependencies]
from_js_ref = "0.2.0"
wasm_refgen = "0.2.0"
```

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

## License

Apache-2.0
