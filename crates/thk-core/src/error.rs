use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ThkError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Network request failed: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("ZIP archive error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Blocked unsafe path traversal attempt: {0}")]
    PathTraversal(PathBuf),

    #[error("Integrity hash mismatch for {file}: expected {expected}, got {actual}")]
    HashMismatch {
        file: String,
        expected: String,
        actual: String,
    },

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Java runtime error: {0}")]
    Java(String),

    #[error("Mod loader error: {0}")]
    Loader(String),

    #[error("Content provider error: {0}")]
    Provider(String),

    #[error("Server manager error: {0}")]
    Server(String),

    #[error("Process execution error: {0}")]
    Process(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Operation cancelled")]
    Cancelled,
}

pub type ThkResult<T> = Result<T, ThkError>;
