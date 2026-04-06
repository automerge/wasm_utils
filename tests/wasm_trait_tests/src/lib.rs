// Integration tests for wasm_trait macros.
//
// These verify that the generated code compiles against real wasm-bindgen
// types. The tests are compile-time assertions — if this crate compiles,
// the macros work correctly.

mod async_trait;
mod implements;
mod js_name_override;
mod qa_edge_cases;
mod realistic_app;
mod static_methods;
mod sync_trait;
