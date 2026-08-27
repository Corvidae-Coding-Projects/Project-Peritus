//! Error boundary for TUI startup, transport, and terminal ownership.

use std::io;

/// A recoverable failure at the interactive client boundary.
#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    /// A local terminal, endpoint, or framing I/O operation failed.
    #[error("I/O failure: {0}")]
    Io(#[from] io::Error),
    /// A checked A3 value or canonical application frame was invalid.
    #[error("application protocol failure: {0}")]
    Protocol(#[from] peritus_app_protocol::AppProtocolError),
    /// The endpoint returned an illegal message for the current connection phase.
    #[error("daemon protocol violation: {0}")]
    ProtocolViolation(String),
    /// A requested client value could not be represented by its nominal type.
    #[error("invalid client value: {0}")]
    InvalidValue(String),
    /// A background client task stopped unexpectedly.
    #[error("client task failed: {0}")]
    Task(String),
}

impl From<tokio::task::JoinError> for TuiError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::Task(error.to_string())
    }
}
