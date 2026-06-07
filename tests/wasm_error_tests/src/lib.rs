// Integration tests for wasm_bindgen_error.
//
// These verify that the generated code compiles against real wasm-bindgen
// and js-sys types. The tests are compile-time assertions — if this crate
// compiles, the macro works correctly with the real dependencies.

mod doc_examples;
mod js_name_override;
mod manual_error_impl;
mod multi_variant_enum;
mod newtype_wrapper;
mod realistic_app;
