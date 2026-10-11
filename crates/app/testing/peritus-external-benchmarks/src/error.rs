//! Stable failures at the external benchmark boundary.

use std::path::PathBuf;

/// External benchmark adapter failure.
#[derive(Debug, thiserror::Error)]
pub enum BenchmarkError {
    /// Command-line input was incomplete or invalid.
    #[error("invalid benchmark command: {0}")]
    Arguments(String),
    /// The external adapter and native report contracts do not match.
    #[error("unsupported external adapter schema {actual}; native schema is {expected}")]
    UnsupportedSchema {
        /// Adapter-declared schema.
        actual: u32,
        /// Native supported schema.
        expected: u32,
    },
    /// Required workspace path is absent before admission.
    #[error("benchmark workspace is missing: {}", path.display())]
    MissingWorkspace {
        /// Missing caller-declared path.
        path: PathBuf,
    },
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
    /// Both primary report publication and the separately prepared recovery path failed.
    #[error(
        "benchmark report publication failed at {}; recovery failed at {}",
        primary.display(),
        recovery.display()
    )]
    ReportPublication {
        /// Intended primary report.
        primary: PathBuf,
        /// Intended recovery report.
        recovery: PathBuf,
        /// Bounded primary publication failure.
        primary_detail: String,
        /// Bounded recovery publication failure.
        recovery_detail: String,
    },
    /// The exactly-once terminal report guard was invoked more than once.
    #[error("benchmark invocation already finalized")]
    DuplicateFinalization,
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

    /// Stable machine-readable failure class retained in terminal evidence.
    #[must_use]
    pub const fn stable_kind(&self) -> &'static str {
        match self {
            Self::Arguments(_) => "arguments",
            Self::UnsupportedSchema { .. } => "unsupported_schema",
            Self::MissingWorkspace { .. } => "missing_workspace",
            Self::Workspace(_) => "workspace",
            Self::Command { .. } => "command",
            Self::Provider(_) => "provider",
            Self::Identity(_) => "identity",
            Self::ReportPublication { .. } => "report_publication",
            Self::DuplicateFinalization => "duplicate_finalization",
            Self::Trace { .. } => "trace",
            Self::Serialization(_) => "serialization",
            Self::Filesystem { .. } => "filesystem",
        }
    }
}
