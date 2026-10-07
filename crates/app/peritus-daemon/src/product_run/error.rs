//! Product-run service and configuration failures.

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};
use peritus_app_protocol::{
    AppDiagnostic, AppErrorCode, AppProtocolError, AppResponsePayload, ResponsibleSubsystem,
    RetryDisposition,
};
use peritus_types::RunId;

/// A live run temporarily cannot inspect the durable authority that governs its next effect.
///
/// The recovery owner is retained with the error so retrying never changes the run, start
/// operation, conversation, or exclusive authority set.
#[derive(Clone)]
pub struct GoverningStateUnavailable {
    run: RunId,
    start: peritus_product_runner::control::ControlOperation,
    authoritative_revision: u64,
    operation: &'static str,
    cause: std::sync::Arc<crate::product_control::ControlStoreError>,
    recovery: std::sync::Arc<crate::product_control::ControlReconciliation>,
}

impl GoverningStateUnavailable {
    pub(super) fn new(
        run: RunId,
        start: peritus_product_runner::control::ControlOperation,
        authoritative_revision: u64,
        operation: &'static str,
        cause: crate::product_control::ControlStoreError,
        recovery: std::sync::Arc<crate::product_control::ControlReconciliation>,
    ) -> Self {
        Self {
            run,
            start,
            authoritative_revision,
            operation,
            cause: std::sync::Arc::new(cause),
            recovery,
        }
    }

    #[must_use]
    pub const fn authoritative_revision(&self) -> u64 {
        self.authoritative_revision
    }

}

impl std::fmt::Debug for GoverningStateUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GoverningStateUnavailable")
            .field("run", &self.run)
            .field("start", &self.start.id())
            .field("conversation", &self.start.conversation())
            .field("authoritative_revision", &self.authoritative_revision)
            .field("operation", &self.operation)
            .field("cause", &self.cause)
            .field("recovery_authorities", &self.recovery.authorities())
            .finish_non_exhaustive()
    }
}

impl PartialEq for GoverningStateUnavailable {
    fn eq(&self, other: &Self) -> bool {
        self.run == other.run
            && self.start == other.start
            && self.authoritative_revision == other.authoritative_revision
            && self.operation == other.operation
            && self.cause.to_string() == other.cause.to_string()
    }
}

impl Eq for GoverningStateUnavailable {}

impl std::fmt::Display for GoverningStateUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: {}. Durable governing revision {} remains authoritative; recovery will retry the same run and control operation",
            self.operation, self.cause, self.authoritative_revision,
        )
    }
}

const MAX_PUBLIC_DIAGNOSTIC_BYTES: usize = 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProductRunServiceError {
    Control(peritus_product_runner::control::ControlError),
    GoverningStateUnavailable(GoverningStateUnavailable),
    Duplicate,
    NotFound,
    ProviderUnavailable,
    EffortUnsupported,
    WorkspaceUnavailable,
    GitRequired,
    InvalidState,
    InvalidMessage,
    Unavailable,
    Context {
        code: AppErrorCode,
        retry: RetryDisposition,
        subsystem: ResponsibleSubsystem,
        operation: &'static str,
        detail: String,
    },
}

impl ProductRunServiceError {
    pub(super) fn persistence(operation: &'static str, error: impl std::fmt::Display) -> Self {
        Self::Context {
            code: AppErrorCode::Internal,
            retry: RetryDisposition::AfterRecovery,
            subsystem: ResponsibleSubsystem::Daemon,
            operation,
            detail: format!(
                "{error}. Peritus stopped the run because its conversation history was not durable. Restore write access to the Peritus state directory, restart Peritus, then retry"
            ),
        }
    }

    pub(super) fn internal(operation: &'static str, detail: impl Into<String>) -> Self {
        Self::Context {
            code: AppErrorCode::Internal,
            retry: RetryDisposition::AfterRecovery,
            subsystem: ResponsibleSubsystem::Daemon,
            operation,
            detail: detail.into(),
        }
    }

    pub(super) fn invalid_data(operation: &'static str, error: impl std::fmt::Display) -> Self {
        Self::Context {
            code: AppErrorCode::MalformedFrame,
            retry: RetryDisposition::NewRequest,
            subsystem: ResponsibleSubsystem::Command,
            operation,
            detail: format!("{error}. Submit a new request with corrected input"),
        }
    }

    pub(super) fn invalid_provider_output(
        operation: &'static str,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::Context {
            code: AppErrorCode::MalformedFrame,
            retry: RetryDisposition::AfterRecovery,
            subsystem: ResponsibleSubsystem::Provider,
            operation,
            detail: format!(
                "{error}. The configured provider returned invalid UTF-8; verify the endpoint and model compatibility before trying again"
            ),
        }
    }

    pub(super) fn describe(&self) -> String {
        match self {
            Self::GoverningStateUnavailable(error) => error.to_string(),
            Self::Context { operation, detail, .. } => format!("{operation}: {detail}"),
            _ => self.default_diagnostic().to_owned(),
        }
    }

    pub(crate) fn response(self) -> AppResponsePayload {
        use peritus_product_runner::control::ControlError;
        let (code, retry, subsystem, diagnostic) = match self {
            Self::GoverningStateUnavailable(error) => {
                return AppResponsePayload::Error(AppProtocolError::classified(
                    AppErrorCode::NotReady,
                    RetryDisposition::AfterRecovery,
                    ResponsibleSubsystem::Daemon,
                    bounded_diagnostic(error.to_string()),
                ));
            }
            Self::Context { code, retry, subsystem, operation, detail } => {
                let diagnostic = bounded_diagnostic(format!("{operation}: {detail}"));
                return AppResponsePayload::Error(AppProtocolError::classified(
                    code, retry, subsystem, diagnostic,
                ));
            }
            Self::Duplicate
            | Self::InvalidState
            | Self::Control(ControlError::IdempotencyConflict) => (
                AppErrorCode::IdempotencyConflict,
                RetryDisposition::NewRequest,
                ResponsibleSubsystem::Command,
                self.default_diagnostic(),
            ),
            Self::NotFound | Self::Control(ControlError::NotFound) => (
                AppErrorCode::InvalidIdentifier,
                RetryDisposition::NewRequest,
                ResponsibleSubsystem::Command,
                self.default_diagnostic(),
            ),
            Self::ProviderUnavailable => (
                AppErrorCode::InvalidIdentifier,
                RetryDisposition::NewRequest,
                ResponsibleSubsystem::Provider,
                self.default_diagnostic(),
            ),
            Self::WorkspaceUnavailable => (
                AppErrorCode::InvalidIdentifier,
                RetryDisposition::NewRequest,
                ResponsibleSubsystem::Workspace,
                self.default_diagnostic(),
            ),
            Self::Control(ControlError::StaleRevision) => (
                AppErrorCode::StaleRevision,
                RetryDisposition::NewRequest,
                ResponsibleSubsystem::Command,
                self.default_diagnostic(),
            ),
            Self::InvalidMessage | Self::Control(ControlError::InvalidInput) => (
                AppErrorCode::MalformedFrame,
                RetryDisposition::NewRequest,
                ResponsibleSubsystem::Command,
                self.default_diagnostic(),
            ),
            Self::Unavailable => (
                AppErrorCode::Internal,
                RetryDisposition::AfterRecovery,
                ResponsibleSubsystem::Daemon,
                self.default_diagnostic(),
            ),
            Self::GitRequired => (
                AppErrorCode::MissingRequiredFeature,
                RetryDisposition::NewRequest,
                ResponsibleSubsystem::Workspace,
                self.default_diagnostic(),
            ),
            Self::EffortUnsupported => (
                AppErrorCode::MissingRequiredFeature,
                RetryDisposition::NewRequest,
                ResponsibleSubsystem::Provider,
                self.default_diagnostic(),
            ),
            Self::Control(ControlError::Capacity) => (
                AppErrorCode::LimitExceeded,
                RetryDisposition::AfterRecovery,
                ResponsibleSubsystem::Daemon,
                self.default_diagnostic(),
            ),
            Self::Control(ControlError::ScopeMismatch) => (
                AppErrorCode::SessionMismatch,
                RetryDisposition::Reconnect,
                ResponsibleSubsystem::Session,
                self.default_diagnostic(),
            ),
            Self::Control(ControlError::UnsupportedSchema) => (
                AppErrorCode::UnsupportedSchema,
                RetryDisposition::Reconnect,
                ResponsibleSubsystem::Negotiation,
                self.default_diagnostic(),
            ),
        };
        AppResponsePayload::Error(AppProtocolError::classified(
            code,
            retry,
            subsystem,
            bounded_diagnostic(diagnostic.to_owned()),
        ))
    }

    const fn default_diagnostic(&self) -> &'static str {
        use peritus_product_runner::control::ControlError;
        match self {
            Self::GoverningStateUnavailable(_) => {
                "The run's governing state is temporarily unavailable. Restore control storage and retry the same run."
            }
            Self::Duplicate => "This run identifier is already in use. Start a new run.",
            Self::NotFound | Self::Control(ControlError::NotFound) => {
                "The requested run no longer exists. Refresh the run list and try again."
            }
            Self::ProviderUnavailable => {
                "The selected provider or model is unavailable. Check provider settings and refresh the model list."
            }
            Self::EffortUnsupported => {
                "The selected reasoning effort is unsupported by this provider. Choose another effort or use the provider default."
            }
            Self::WorkspaceUnavailable => {
                "The workspace is unavailable. Reopen an existing readable workspace and try again."
            }
            Self::GitRequired => {
                "This operation requires a Git workspace. Initialize Git or open a Git repository."
            }
            Self::InvalidState | Self::Control(ControlError::IdempotencyConflict) => {
                "The request conflicts with the run's current state. Refresh the run before retrying."
            }
            Self::InvalidMessage | Self::Control(ControlError::InvalidInput) => {
                "The request contains invalid conversation or command data. Correct it and submit a new request."
            }
            Self::Unavailable => {
                "The daemon could not access required run state. Inspect the daemon log, restart Peritus, and retry."
            }
            Self::Control(ControlError::StaleRevision) => {
                "The request used an outdated revision. Refresh the conversation and submit a new request."
            }
            Self::Control(ControlError::Capacity) => {
                "The run reached a configured capacity limit. Finish or remove existing work before retrying."
            }
            Self::Control(ControlError::ScopeMismatch) => {
                "The request belongs to a different workspace or session. Reopen the workspace and retry."
            }
            Self::Control(ControlError::UnsupportedSchema) => {
                "Stored control state uses an unsupported schema. Upgrade Peritus or reconcile the stored state."
            }
            Self::Context { .. } => "The operation failed.",
        }
    }
}
impl From<crate::product_control::ControlStoreError> for ProductRunServiceError {
    fn from(value: crate::product_control::ControlStoreError) -> Self {
        use crate::product_control::ControlStoreError;
        match value {
            ControlStoreError::Control(error) => Self::Control(error),
            ControlStoreError::Journal(error) => Self::internal(
                "access the durable control journal",
                format!("{error}. Reconcile the control journal before admitting more effects"),
            ),
            ControlStoreError::Io(error) => Self::internal(
                "access durable control storage",
                format!("{error}. Check state-directory ownership and whether another daemon is running, then restart Peritus"),
            ),
            ControlStoreError::ContentionCancelled => Self::Context {
                code: AppErrorCode::Backpressure,
                retry: RetryDisposition::NewRequest,
                subsystem: ResponsibleSubsystem::Daemon,
                operation: "wait for the durable control journal",
                detail: "The journal owner cancelled this wait. The original command identity remains available for exact reconciliation".to_owned(),
            },
            ControlStoreError::Workspace(error) => Self::internal(
                "apply the workspace operation",
                format!("{error}. Inspect the workspace and permissions before retrying"),
            ),
            ControlStoreError::Runner(error) => Self::internal(
                "authorize the workspace operation",
                format!("{error}. Inspect the run state before retrying"),
            ),
            ControlStoreError::PermissionDenied => Self::Context {
                code: AppErrorCode::ReadOnly,
                retry: RetryDisposition::AfterRecovery,
                subsystem: ResponsibleSubsystem::Command,
                operation: "authorize the workspace operation",
                detail: "Workspace writes are disabled by the effective permission policy. Review /permissions before retrying".to_owned(),
            },
            ControlStoreError::Corrupt(detail) => Self::internal(
                "validate durable control state",
                format!("{detail}. Reconcile the stored control state before retrying"),
            ),
            ControlStoreError::StalePreimage => Self::Context {
                code: AppErrorCode::NotReady,
                retry: RetryDisposition::NewRequest,
                subsystem: ResponsibleSubsystem::Workspace,
                operation: "inspect checkpoint target",
                detail: "The workspace target changed during inspection. Reobserve its current contents before mutation.".to_owned(),
            },
        }
    }
}

impl std::fmt::Display for ProductRunServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.describe())
    }
}

impl std::error::Error for ProductRunServiceError {}

fn bounded_diagnostic(mut value: String) -> Option<AppDiagnostic> {
    if value.len() > MAX_PUBLIC_DIAGNOSTIC_BYTES {
        let mut end = MAX_PUBLIC_DIAGNOSTIC_BYTES.saturating_sub(3);
        while !value.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        value.truncate(end);
        value.push_str("...");
    }
    AppDiagnostic::new(value, MAX_PUBLIC_DIAGNOSTIC_BYTES).ok()
}

pub(super) fn filesystem(error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "access product-run state",
        "product-run state is unavailable",
        error,
    )
}

pub(super) fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "configure product runs",
        detail,
    )
}

#[cfg(test)]
mod tests;
