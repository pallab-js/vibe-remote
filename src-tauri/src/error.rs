//! Error types for VibeRemote
//!
//! This module defines the centralized error handling strategy using `thiserror`
//! for error type definitions and `anyhow` for error propagation in application code.

use thiserror::Error;

/// Main error type for VibeRemote operations
#[derive(Error, Debug)]
pub enum VibeError {
    #[error("Capture error: {0}")]
    Capture(String),

    #[error("QUIC error: {0}")]
    Quic(#[from] quinn::ConnectError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Input error: {0}")]
    Input(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// Result type alias for VibeRemote operations
pub type VibeResult<T> = Result<T, VibeError>;

/// Convert anyhow errors to VibeError
impl From<anyhow::Error> for VibeError {
    fn from(err: anyhow::Error) -> Self {
        VibeError::Capture(err.to_string())
    }
}
