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
    PermissionDenied,
    Corrupt(&'static str),
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
            Self::PermissionDenied => {
                f.write_str("workspace writes are disabled by effective policy")
            }
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
            Self::Corrupt(_) | Self::PermissionDenied => None,
        }
    }
}
