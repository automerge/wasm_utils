//! Runtime Wasm tests for the `wasm_utils` workspace.
//!
//! Unlike the token-level and compile-time integration tests in the other
//! `tests/` crates, these execute inside a real Wasm/JS runtime via
//! `wasm-bindgen-test`. They verify behavior that only manifests at runtime:
//!
//! - `FromJsRef::try_from_js_value` duck-typing via `Reflect::has`
//! - the injected upcast clone method actually resolving on a JS object
//! - `WasmError` producing a JS `Error` with the right `.name`/`.message`
//! - `#[js_trait]` sync delegation and async `JsFuture` resolve/reject
//! - `#[wasm_implements]` types being callable across the boundary
//!
//! Run with `wasm-pack test --node tests/wasm_runtime_tests`. The suite is
//! configured for Node; to run it in a browser, set `WASM_BINDGEN_USE_BROWSER=1`:
//!
//! ```sh
//! WASM_BINDGEN_USE_BROWSER=1 wasm-pack test --headless --firefox tests/wasm_runtime_tests
//! ```
//!
//! Without it, `--firefox`/`--chrome` runs skip every test and still exit 0.

#![allow(
    clippy::expect_used,
    clippy::needless_pass_by_value,
    clippy::new_without_default,
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs
)]

pub mod error_runtime;
pub mod refgen_runtime;
pub mod trait_runtime;
