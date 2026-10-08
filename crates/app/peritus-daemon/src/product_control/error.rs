//! Typed storage failures with preserved sources and redaction-safe public descriptions.

use peritus_journal::JournalError;
use peritus_product_runner::control::ControlError;

#[derive(Debug)]
pub enum ControlStoreError {
    Control(ControlError),
    Journal(JournalError),
    Io(std::io::Error),
    Workspace(peritus_workspace::WorkspaceError),
    Runner(peritus_product_runner::ProductRunnerError),
    ContentionCancelled,
    PermissionDenied,
    StalePreimage,
    Corrupt(&'static str),
}

impl ControlStoreError {
    pub(crate) fn is_storage_exhausted(&self) -> bool {
        let mut next: Option<&(dyn std::error::Error + 'static)> = Some(self);
        while let Some(error) = next {
            if error.downcast_ref::<JournalError>().is_some_and(JournalError::is_storage_exhausted)
                || error.downcast_ref::<rusqlite::Error>().is_some_and(|error| {
                    matches!(error, rusqlite::Error::SqliteFailure(failure, _)
                        if failure.code == rusqlite::ErrorCode::DiskFull)
                })
                || error.downcast_ref::<std::io::Error>().is_some_and(|error| {
                    matches!(
                        error.kind(),
                        std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded
                    )
                })
            {
                return true;
            }
            // io::Error::source forwards to the wrapped error's source, which can skip the
            // actual SQLite failure. Inspect the owned inner error before following its chain.
            next = error
                .downcast_ref::<std::io::Error>()
                .and_then(std::io::Error::get_ref)
                .map(|inner| inner as &(dyn std::error::Error + 'static))
                .or_else(|| error.source());
        }
        false
    }
}

impl From<ControlError> for ControlStoreError {
    fn from(error: ControlError) -> Self {
        Self::Control(error)
    }
}
impl From<JournalError> for ControlStoreError {
    fn from(error: JournalError) -> Self {
        Self::Journal(error)
    }
}
impl From<std::io::Error> for ControlStoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<peritus_workspace::WorkspaceError> for ControlStoreError {
    fn from(error: peritus_workspace::WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}
impl From<peritus_product_runner::ProductRunnerError> for ControlStoreError {
    fn from(error: peritus_product_runner::ProductRunnerError) -> Self {
        Self::Runner(error)
    }
}
impl std::fmt::Display for ControlStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Control(error) => error.fmt(f),
            Self::Journal(_) => {
                f.write_str("control journal unavailable; reconcile before admitting effects")
            }
            Self::Io(_) => f.write_str("control storage unavailable or already owned"),
            Self::Workspace(_) => f.write_str("workspace mutation failed; inspect before retrying"),
            Self::Runner(_) => f.write_str("workspace mutation authority unavailable"),
            Self::ContentionCancelled => {
                f.write_str("control journal contention wait was cancelled")
            }
            Self::PermissionDenied => {
                f.write_str("workspace writes are disabled by effective policy")
            }
            Self::StalePreimage => f.write_str(
                "workspace target changed during checkpoint inspection; reobserve before mutation",
            ),
            Self::Corrupt(detail) => write!(f, "control integrity failure: {detail}"),
        }
    }
}
impl std::error::Error for ControlStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Control(error) => Some(error),
            Self::Journal(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Workspace(error) => Some(error),
            Self::Runner(error) => Some(error),
            Self::ContentionCancelled
            | Self::Corrupt(_)
            | Self::PermissionDenied
            | Self::StalePreimage => None,
        }
    }
}
