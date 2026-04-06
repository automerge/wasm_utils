# wasm_trait

JS duck-typed interfaces as Rust traits with compile-time conformance checking.

## Macros

### `#[js_trait]`

Define a JS interface as a Rust trait. Generates an `extern "C"` block, a
TypeScript interface, and an `impl Trait for ExternType`.

```rust
use wasm_trait::js_trait;

#[js_trait(js_type = JsTransport)]
pub trait Transport {
    #[wasm_bindgen(js_name = "sendBytes")]
    async fn js_send_bytes(&self, bytes: Uint8Array) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = "recvBytes")]
    async fn js_recv_bytes(&self) -> Result<Uint8Array, JsValue>;

    #[wasm_bindgen(js_name = "onDisconnect")]
    fn js_on_disconnect(&self, callback: Function);
}
```

### `#[wasm_implements]`

Compile-time check that a `#[wasm_bindgen]` impl block conforms to a trait.

```rust
use wasm_trait::wasm_implements;

#[wasm_implements(Transport)]
#[wasm_bindgen(js_class = "SubductionHttpLongPoll")]
impl WasmHttpLongPoll {
    #[wasm_bindgen(js_name = "sendBytes")]
    pub async fn js_send_bytes(&self, bytes: Uint8Array) -> Result<(), JsValue> {
        // ...
    }

    #[wasm_bindgen(js_name = "recvBytes")]
    pub async fn js_recv_bytes(&self) -> Result<Uint8Array, JsValue> {
        // ...
    }

    #[wasm_bindgen(js_name = "onDisconnect")]
    pub fn js_on_disconnect(&self, callback: Function) {
        // ...
    }
}
```

## Parameters

### `#[js_trait]`

| Parameter | Required | Default    | Description |
|-----------|----------|------------|-------------|
| `js_type` | Yes      | —          | Rust ident for the generated extern type |
| `js_name` | No       | Trait name | JS/TS interface name |

### `#[wasm_implements]`

Takes a single argument: the trait path to check against.

## License

Apache-2.0
