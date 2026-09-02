//! Typed failures at the reviewed H0 controller boundary.

use std::path::Path;

/// Failure to authenticate, execute, or publish one native security probe.
#[derive(Debug, thiserror::Error)]
pub(super) enum ControllerError {
    /// The fixed command-line interface was not followed exactly.
    #[error("{0}")]
    Arguments(&'static str),
    /// A bounded protocol or candidate-binding invariant failed.
    #[error("H0 controller protocol: {0}")]
    Protocol(String),
    /// A filesystem operation failed.
    #[error("H0 controller {operation} `{path}`: {source}")]
    Io {
        operation: &'static str,
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// A JSON protocol or evidence document could not be encoded or decoded.
    #[error("H0 controller JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// A checked TOML inventory could not be decoded.
    #[error("H0 controller inventory: {0}")]
    Toml(#[from] toml::de::Error),
    /// The exact committed candidate could not be inspected.
    #[error(transparent)]
    Repository(#[from] crate::repository::RepositoryError),
}

impl ControllerError {
    pub(super) fn protocol(detail: impl Into<String>) -> Self {
        Self::Protocol(detail.into())
    }

    pub(super) fn io(operation: &'static str, path: &Path, source: std::io::Error) -> Self {
        Self::Io { operation, path: path.display().to_string(), source }
    }
}
