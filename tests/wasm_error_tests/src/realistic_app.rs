//! Realistic application pattern — shows how `WasmError` fits into
//! a typical wasm-bindgen exported API with `Result` return types.

use thiserror::Error;
use wasm_bindgen::prelude::*;
use wasm_bindgen_error::WasmError;

// ── Domain errors (from a core library) ─────────────────────────

#[derive(Debug, Error)]
#[error("document `{id}` not found")]
pub struct NotFoundError {
    id: String,
}

#[derive(Debug, Error)]
#[error("validation failed: {reason}")]
pub struct ValidationError {
    reason: String,
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("read failed: {0}")]
    Read(String),

    #[error("write failed: {0}")]
    Write(String),
}

// ── Wasm error wrappers ─────────────────────────────────────────

/// JS sees `"NotFoundError"`.
#[derive(Debug, Error, WasmError)]
#[error(transparent)]
pub struct WasmNotFoundError(#[from] NotFoundError);

/// JS sees `"ValidationError"`.
#[derive(Debug, Error, WasmError)]
#[error(transparent)]
pub struct WasmValidationError(#[from] ValidationError);

/// Multiple failure modes funneled into one JS error.
/// JS sees `"WriteError"`.
#[derive(Debug, Error, WasmError)]
pub enum WasmWriteError {
    #[error("validation: {0}")]
    Validation(#[from] ValidationError),

    #[error("storage: {0}")]
    Storage(#[from] StorageError),
}

// ── Exported Wasm API ───────────────────────────────────────────

#[wasm_bindgen]
pub struct DocumentStore;

// A realistic impl block showing how `?` flows through the error chain.
// This can't use `#[wasm_bindgen]` on methods in a test crate (no JS
// runtime), but we verify the types line up for `Result<_, WasmXxxError>`.

impl DocumentStore {
    /// Simulates a lookup that can fail with `WasmNotFoundError`.
    pub fn get_document(&self, id: &str) -> Result<String, WasmNotFoundError> {
        if id.is_empty() {
            return Err(NotFoundError { id: id.to_string() })?;
        }
        Ok(format!("doc:{id}"))
    }

    /// Simulates a write that can fail with `WasmWriteError`.
    pub fn save_document(&self, id: &str, content: &str) -> Result<(), WasmWriteError> {
        if content.is_empty() {
            return Err(ValidationError {
                reason: "content must not be empty".into(),
            })?;
        }
        if id.starts_with("readonly:") {
            return Err(StorageError::Write(format!("{id} is read-only")))?;
        }
        Ok(())
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[test]
fn all_error_types_convert_to_jsvalue() {
    fn assert_from<T: Into<JsValue>>() {}
    assert_from::<WasmNotFoundError>();
    assert_from::<WasmValidationError>();
    assert_from::<WasmWriteError>();
}

#[test]
fn question_mark_chains_work() {
    let store = DocumentStore;

    let err = store.get_document("").unwrap_err();
    assert!(err.to_string().contains("not found"));

    let err = store.save_document("doc:1", "").unwrap_err();
    assert!(err.to_string().contains("content must not be empty"));

    let err = store.save_document("readonly:config", "data").unwrap_err();
    assert!(err.to_string().contains("read-only"));
}

#[test]
fn ok_paths_work() {
    let store = DocumentStore;
    assert!(store.get_document("doc:1").is_ok());
    assert!(store.save_document("doc:1", "hello").is_ok());
}
