//! Actionable launcher failures.

use std::path::PathBuf;

/// Failure at the local product-composition boundary.
#[derive(Debug, thiserror::Error)]
pub enum LauncherError {
    /// Platform application directories cannot be determined safely.
    #[error("cannot determine Peritus application directories: {0}")]
    PlatformPaths(String),
    /// A filesystem operation failed at one exact path.
    #[error("{operation} failed for {}: {source}", path.display())]
    Filesystem {
        /// Stable operation description.
        operation: &'static str,
        /// Exact affected path.
        path: PathBuf,
        /// Underlying operating-system failure.
        #[source]
        source: std::io::Error,
    },
    /// Stable installation identity generation failed.
    #[error("could not generate local installation identity: {0}")]
    Random(String),
    /// Another launcher currently owns local bootstrap publication.
    #[error("another Peritus process is currently preparing local application state")]
    BootstrapBusy,
    /// Pure product state is invalid.
    #[error("local product state is invalid: {0}")]
    ProductState(#[from] peritus_product_state::ProductStateError),
    /// Public approval-registry construction or validation failed.
    #[error("public approval registry is invalid: {0}")]
    Approval(String),
    /// Generated strict daemon configuration is invalid.
    #[error("generated daemon configuration is invalid: {0}")]
    DaemonConfig(#[from] peritus_daemon::DaemonError),
    /// The installed daemon binary cannot be resolved.
    #[error("packaged daemon executable is unavailable: {0}")]
    DaemonBinary(String),
    /// The daemon process could not be started.
    #[error("could not start packaged daemon: {0}")]
    DaemonSpawn(String),
    /// The daemon exited before publishing readiness.
    #[error("daemon exited before readiness with status {status}; diagnostics: {}", log.display())]
    DaemonExited {
        /// Native child exit status.
        status: std::process::ExitStatus,
        /// Retained daemon diagnostic log.
        log: PathBuf,
    },
    /// Readiness was not established within the bounded startup interval.
    #[error("daemon did not become ready within {seconds} seconds; diagnostics: {}", log.display())]
    DaemonTimeout {
        /// Configured startup interval.
        seconds: u64,
        /// Retained daemon diagnostic log.
        log: PathBuf,
    },
    /// The terminal application failed after daemon readiness.
    #[error("interactive application failed: {0}")]
    Tui(#[source] peritus_tui::TuiError),
}

impl LauncherError {
    pub(crate) fn filesystem(
        operation: &'static str,
        path: impl Into<PathBuf>,
        source: std::io::Error,
    ) -> Self {
        Self::Filesystem { operation, path: path.into(), source }
    }
}

impl From<peritus_approval::ApprovalError> for LauncherError {
    fn from(error: peritus_approval::ApprovalError) -> Self {
        Self::Approval(format!("{error:?}"))
    }
}
