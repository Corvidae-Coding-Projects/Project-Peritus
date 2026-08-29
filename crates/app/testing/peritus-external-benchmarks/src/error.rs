//! Stable failures at the external benchmark boundary.

use std::path::PathBuf;

/// External benchmark adapter failure.
#[derive(Debug, thiserror::Error)]
pub enum BenchmarkError {
    /// Command-line input was incomplete or invalid.
    #[error("invalid benchmark command: {0}")]
    Arguments(String),
    /// A required path or workspace invariant was invalid.
    #[error("benchmark workspace is invalid: {0}")]
    Workspace(String),
    /// A local command used to establish the benchmark boundary failed.
    #[error("{operation} failed with {status}: {detail}")]
    Command {
        /// Stable operation name.
        operation: &'static str,
        /// Process exit description.
        status: String,
        /// Bounded diagnostic.
        detail: String,
    },
    /// Provider discovery or authentication failed.
    #[error("benchmark provider setup failed: {0}")]
    Provider(String),
    /// The executable cannot prove the source and binary identity required for retained evidence.
    #[error("benchmark agent identity is invalid: {0}")]
    Identity(String),
    /// The durable provider/tool trace was malformed or unavailable.
    #[error("benchmark trace failed at {}: {detail}", path.display())]
    Trace {
        /// Trace path.
        path: PathBuf,
        /// Redaction-safe diagnostic.
        detail: String,
    },
    /// Evidence serialization failed.
    #[error("benchmark evidence serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    /// A filesystem effect failed.
    #[error("benchmark filesystem operation {operation} failed at {}: {source}", path.display())]
    Filesystem {
        /// Stable operation name.
        operation: &'static str,
        /// Affected path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

impl BenchmarkError {
    pub(crate) fn filesystem(
        operation: &'static str,
        path: impl Into<PathBuf>,
        source: std::io::Error,
    ) -> Self {
        Self::Filesystem { operation, path: path.into(), source }
    }

    pub(crate) fn trace(path: impl Into<PathBuf>, detail: impl Into<String>) -> Self {
        Self::Trace { path: path.into(), detail: detail.into() }
    }
}
