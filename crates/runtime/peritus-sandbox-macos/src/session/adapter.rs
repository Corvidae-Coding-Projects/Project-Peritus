//! C2 native-session trait adapter and stable error translation.

use peritus_process::{
    CancellationReason, ErrorCode, NativeLaunchDescription, NativePoll, NativeSandboxSession,
    OsExitObservation, ProcessError, ProcessOperation, ProcessTreeIdentity,
    RecoveryClass as ProcessRecovery,
};
use peritus_sandbox::EnforcementObservation;

use super::{MacosSession, SessionPhase};
use crate::{MacosError, MacosErrorKind, MacosOperation, RecoveryAction};

impl NativeSandboxSession for MacosSession {
    fn launch_description(&self) -> &NativeLaunchDescription {
        &self.launch
    }

    fn observations(&self) -> &[EnforcementObservation] {
        &self.observations
    }

    fn poll_resources(&mut self, tree: ProcessTreeIdentity) -> Result<NativePoll, ProcessError> {
        if !matches!(self.phase, SessionPhase::Active | SessionPhase::Cancelling) {
            return Err(process_error(&lifecycle_error(
                "resource polling requires an active or cancelling session",
            )));
        }
        let identity = self.recovery.identity();
        if identity.root_pid() != Some(tree.root_pid())
            || identity.process_group() != tree.process_group()
            || !tree.complete_containment()
        {
            return Err(process_error(&lifecycle_error(
                "resource poll process-tree identity differs from activation",
            )));
        }
        self.resource_monitor
            .poll(tree, self.manifest.resources())
            .map(
                |exceeded| {
                    if exceeded { NativePoll::ResourceLimitExceeded } else { NativePoll::Continue }
                },
            )
            .map_err(|error| process_error(&error))
    }

    fn activated(&mut self, tree: ProcessTreeIdentity) -> Result<(), ProcessError> {
        self.record_activation(tree).map_err(|error| process_error(&error))
    }

    fn cancellation_requested(&mut self, reason: CancellationReason) -> Result<(), ProcessError> {
        self.record_cancellation(reason).map_err(|error| process_error(&error))
    }

    fn terminated(&mut self, exit: &OsExitObservation) -> Result<(), ProcessError> {
        self.record_termination(exit).map_err(|error| process_error(&error))
    }

    fn release(&mut self) -> Result<(), ProcessError> {
        self.record_release().map(|_| ()).map_err(|error| process_error(&error))
    }
}

pub(super) fn lifecycle_error(detail: &'static str) -> MacosError {
    MacosError::new(
        MacosErrorKind::ObservationMismatch,
        MacosOperation::Validate,
        RecoveryAction::Reconcile,
        detail,
    )
}

pub(crate) const fn process_error(error: &MacosError) -> ProcessError {
    let (code, operation, recovery) = match error.kind() {
        MacosErrorKind::InvalidInput | MacosErrorKind::LimitExceeded => {
            (ErrorCode::InvalidInput, ProcessOperation::Validate, ProcessRecovery::CorrectRequest)
        }
        MacosErrorKind::UnsupportedHost => {
            (ErrorCode::Unsupported, ProcessOperation::Validate, ProcessRecovery::SelectBackend)
        }
        MacosErrorKind::DescriptorMismatch | MacosErrorKind::PreparationMismatch => {
            (ErrorCode::PlanMismatch, ProcessOperation::Validate, ProcessRecovery::Reauthorize)
        }
        MacosErrorKind::ResourceLimit => {
            (ErrorCode::ResourceLimit, ProcessOperation::Control, ProcessRecovery::CancelAndReap)
        }
        MacosErrorKind::RecoveryIndeterminate => {
            (ErrorCode::Indeterminate, ProcessOperation::Reconcile, ProcessRecovery::Quarantine)
        }
        _ => (ErrorCode::Supervisor, ProcessOperation::Wait, ProcessRecovery::CancelAndReap),
    };
    let detail = match error.kind() {
        MacosErrorKind::InvalidInput => "macOS backend input is invalid",
        MacosErrorKind::LimitExceeded => "macOS backend bound was exceeded",
        MacosErrorKind::UnsupportedHost => "macOS host lacks a required native control",
        MacosErrorKind::ProbeFailed => "macOS capability probe failed",
        MacosErrorKind::DescriptorMismatch => "macOS descriptor differs from admission",
        MacosErrorKind::PreparationMismatch => "macOS preparation identity differs",
        MacosErrorKind::ProfileCompilation => "macOS Seatbelt profile cannot be represented",
        MacosErrorKind::HelperFailure => "macOS helper failed before target completion",
        MacosErrorKind::SandboxDenied => "macOS Seatbelt activation was denied",
        MacosErrorKind::ResourceLimit => "macOS resource enforcement failed",
        MacosErrorKind::SupervisorFailure => "macOS process supervision failed",
        MacosErrorKind::ObservationMismatch => "macOS lifecycle observation is invalid",
        MacosErrorKind::CleanupIncomplete => "macOS native cleanup is incomplete",
        MacosErrorKind::RecoveryIndeterminate => "macOS native recovery is indeterminate",
        MacosErrorKind::Io => "macOS native I/O failed",
    };
    ProcessError::new(code, operation, recovery, detail)
}
