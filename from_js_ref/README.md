# `from_js_ref`

Runtime traits for converting between JS-imported reference types and Rust-exported `wasm-bindgen` types.

This crate is the companion to [`wasm_refgen`](https://crates.io/crates/wasm_refgen), which generates implementations of these traits via a proc-macro.

## Traits

### `FromJsRef`

```rust
pub trait FromJsRef: Sized {
    type JsRef: JsCast;

    /// Convert from a JS reference type to the Rust type.
    fn from_js_ref(castable: &Self::JsRef) -> Self;

    /// Attempt to convert from a raw `JsValue`.
    ///
    /// Provided by default using `dyn_ref` (instanceof).
    /// `wasm_refgen` overrides this with a duck-type check via
    /// `Reflect::has` for reliable validation.
    fn try_from_js_value(js_value: &JsValue) -> Option<Self> { /* ... */ }
}
```

### `JsDeref`

A convenience trait blanket-implemented for all `T::JsRef` types:

```rust
let wasm_foo: WasmFoo = js_foo.js_deref();
```

## Usage

You typically don't implement these traits by hand. Instead, use
`#[wasm_refgen(js_ref = JsFoo)]` on your `impl` block, which generates
the `FromJsRef` implementation automatically.

```rust
use from_js_ref::FromJsRef;

// From a typed reference (infallible)
let foo: WasmFoo = WasmFoo::from_js_ref(&js_foo);

// From a raw JsValue (fallible, duck-type validated)
let foo: Option<WasmFoo> = WasmFoo::try_from_js_value(&js_value);
```

> [!WARNING]
> Do _not_ use `dyn_into::<JsFoo>()` or `dyn_ref::<JsFoo>()` with
> `wasm_refgen`-generated types. These rely on `instanceof`, which
> targets the Rust identifier name rather than the JS class name.
> Use `FromJsRef::try_from_js_value` instead.

## License

Apache-2.0
