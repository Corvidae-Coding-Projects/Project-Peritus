//! Product-run service and configuration failures.

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};
use peritus_app_protocol::{
    AppDiagnostic, AppErrorCode, AppProtocolError, AppResponsePayload, ResponsibleSubsystem,
    RetryDisposition,
};

const MAX_PUBLIC_DIAGNOSTIC_BYTES: usize = 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProductRunServiceError {
    Control(peritus_product_runner::control::ControlError),
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
            Self::Context { operation, detail, .. } => format!("{operation}: {detail}"),
            _ => self.default_diagnostic().to_owned(),
        }
    }

    pub(crate) fn response(self) -> AppResponsePayload {
        use peritus_product_runner::control::ControlError;
        let (code, retry, subsystem, diagnostic) = match self {
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
mod tests {
    use super::*;

    #[test]
    fn unavailable_run_state_is_not_mislabeled_as_subscription_backpressure() {
        let AppResponsePayload::Error(error) = ProductRunServiceError::Unavailable.response()
        else {
            panic!("error response");
        };
        assert_eq!(error.code(), AppErrorCode::Internal);
        assert_eq!(error.subsystem(), ResponsibleSubsystem::Daemon);
        assert_eq!(error.retry(), RetryDisposition::AfterRecovery);
        assert!(error.diagnostic().unwrap().as_str().contains("daemon log"));
    }

    #[test]
    fn persistence_error_keeps_operation_cause_and_recovery_action() {
        let error = ProductRunServiceError::persistence(
            "replace the durable product-run record",
            std::io::Error::from_raw_os_error(13),
        );
        let message = error.describe();
        assert!(message.contains("replace the durable product-run record"));
        assert!(message.contains("Permission denied"));
        assert!(message.contains("Restore write access"));
    }

    #[test]
    fn provider_and_workspace_failures_keep_distinct_public_ownership() {
        for (failure, subsystem) in [
            (ProductRunServiceError::ProviderUnavailable, ResponsibleSubsystem::Provider),
            (ProductRunServiceError::WorkspaceUnavailable, ResponsibleSubsystem::Workspace),
        ] {
            let AppResponsePayload::Error(error) = failure.response() else {
                panic!("error response");
            };
            assert_eq!(error.code(), AppErrorCode::InvalidIdentifier);
            assert_eq!(error.retry(), RetryDisposition::NewRequest);
            assert_eq!(error.subsystem(), subsystem);
        }
    }

    #[test]
    fn product_prerequisite_failures_keep_distinct_public_ownership() {
        for (failure, subsystem, diagnostic) in [
            (
                ProductRunServiceError::GitRequired,
                ResponsibleSubsystem::Workspace,
                "requires a Git workspace",
            ),
            (
                ProductRunServiceError::EffortUnsupported,
                ResponsibleSubsystem::Provider,
                "unsupported by this provider",
            ),
        ] {
            let AppResponsePayload::Error(error) = failure.response() else {
                panic!("error response");
            };
            assert_eq!(error.code(), AppErrorCode::MissingRequiredFeature);
            assert_eq!(error.retry(), RetryDisposition::NewRequest);
            assert_eq!(error.subsystem(), subsystem);
            assert!(error.diagnostic().unwrap().as_str().contains(diagnostic));
        }
    }

    #[test]
    fn invalid_provider_output_is_not_mislabeled_as_command_input() {
        let error = ProductRunServiceError::invalid_provider_output(
            "decode streamed assistant text",
            "source decoder rejected invalid bytes",
        );
        let AppResponsePayload::Error(error) = error.response() else {
            panic!("error response");
        };
        assert_eq!(error.code(), AppErrorCode::MalformedFrame);
        assert_eq!(error.retry(), RetryDisposition::AfterRecovery);
        assert_eq!(error.subsystem(), ResponsibleSubsystem::Provider);
        assert!(error.diagnostic().unwrap().as_str().contains("invalid UTF-8"));
    }

    #[test]
    fn unsupported_control_schema_requires_a_new_protocol_relationship() {
        let AppResponsePayload::Error(error) = ProductRunServiceError::Control(
            peritus_product_runner::control::ControlError::UnsupportedSchema,
        )
        .response() else {
            panic!("error response");
        };
        assert_eq!(error.code(), AppErrorCode::UnsupportedSchema);
        assert_eq!(error.retry(), RetryDisposition::Reconnect);
        assert_eq!(error.subsystem(), ResponsibleSubsystem::Negotiation);
    }
}
