//! C2 native-session trait adapter and stable error translation.

use peritus_process::{
    CancellationReason, ErrorCode, NATIVE_OBSERVATION_PAGE_RECORDS, NativeLaunchDescription,
    NativeObservationPage, NativeObservationReceipt, NativeObservationTransport, NativePlatform,
    NativePoll, NativeRecoveryPhase, NativeSandboxSession, NativeSessionRecovery,
    OsExitObservation, ProcessError, ProcessOperation, ProcessTreeIdentity,
    RecoveryClass as ProcessRecovery,
    native_observation_prefix_digest, native_observation_producer_binding,
};
use peritus_sandbox::EnforcementObservation;

use super::{MacosSession, SessionPhase};
use crate::{MacosError, MacosErrorKind, MacosOperation, RecoveryAction};

impl NativeSandboxSession for MacosSession {
    fn launch_description(&self) -> &NativeLaunchDescription {
        &self.launch
    }

    fn recovery_snapshot(&self) -> Result<Option<NativeSessionRecovery>, ProcessError> {
        let identity = self.recovery.identity();
        let tree = match (
            identity.root_pid(),
            identity.root_start_token(),
            identity.process_group(),
        ) {
            (Some(root), Some(start), Some(group)) => {
                Some(ProcessTreeIdentity::new(root, Some(start), Some(group), true))
            }
            (None, None, None) => None,
            _ => {
                return Err(process_error(&MacosError::new(
                    MacosErrorKind::RecoveryIndeterminate,
                    MacosOperation::Recover,
                    RecoveryAction::Quarantine,
                    "macOS recovery birth identity is incomplete",
                )));
            }
        };
        let phase = match self.recovery.phase() {
            SessionPhase::Prepared => NativeRecoveryPhase::Prepared,
            SessionPhase::Active => NativeRecoveryPhase::Active,
            SessionPhase::Cancelling => NativeRecoveryPhase::Cancelling,
            SessionPhase::Terminated => NativeRecoveryPhase::Terminated,
            SessionPhase::Released => NativeRecoveryPhase::Released,
        };
        let custody = self.recovery.custody();
        let bytes = self.recovery.canonical_bytes();
        let mut record = Vec::new();
        record.try_reserve_exact(bytes.len()).map_err(|_| {
            process_error(&MacosError::new(
                MacosErrorKind::LimitExceeded,
                MacosOperation::Recover,
                RecoveryAction::Reconcile,
                "macOS recovery record cannot be transferred",
            ))
        })?;
        record.extend_from_slice(bytes);
        NativeSessionRecovery::new(
            NativePlatform::Macos,
            identity.process_id(),
            phase,
            tree,
            custody.owner_operation_digest(),
            custody.service_owner_digest(),
            custody.adoptable(self.recovery.phase()),
            record,
        )
        .map(Some)
    }

    fn spawned(&mut self, tree: ProcessTreeIdentity) -> Result<(), ProcessError> {
        self.record_spawned(tree).map_err(|error| process_error(&error))
    }

    fn observations(&self) -> &[EnforcementObservation] {
        self.observations.as_slice()
    }

    fn observation_tail_dropped(&self) -> u64 {
        self.observations.dropped()
    }

    fn acknowledged_observation_receipt(&self) -> Option<NativeObservationReceipt> {
        self.acknowledged_observations
    }

    fn observation_transport(&self) -> NativeObservationTransport {
        NativeObservationTransport::DurableDelta
    }

    fn observation_page(
        &self,
        after_sequence: u64,
    ) -> Result<NativeObservationPage, ProcessError> {
        let acknowledged = self
            .acknowledged_observations
            .map_or(0, NativeObservationReceipt::through_sequence);
        if after_sequence != acknowledged {
            return Err(process_error(&lifecycle_error(
                "native observation page does not begin at the acknowledged frontier",
            )));
        }
        let page = self
            .pending_observations
            .iter()
            .copied()
            .take(NATIVE_OBSERVATION_PAGE_RECORDS)
            .collect();
        NativeObservationPage::new(self.next_observation_sequence - 1, page)
    }

    fn acknowledge_observations(
        &mut self,
        receipt: NativeObservationReceipt,
    ) -> Result<(), ProcessError> {
        if self.acknowledged_observations == Some(receipt) {
            return Ok(());
        }
        let acknowledged = self
            .acknowledged_observations
            .map_or(0, NativeObservationReceipt::through_sequence);
        let count = receipt
            .through_sequence()
            .checked_sub(acknowledged)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|count| *count > 0 && *count <= self.pending_observations.len())
            .ok_or_else(|| process_error(&lifecycle_error(
                "native observation receipt is outside the pending prefix",
            )))?;
        let pending_prefix = self
            .pending_observations
            .iter()
            .copied()
            .take(count)
            .collect::<Vec<_>>();
        let producer_binding = native_observation_producer_binding(
            self.launch.manifest_digest(),
            self.launch.preparation_digest(),
        );
        let previous_prefix = self
            .acknowledged_observations
            .map(NativeObservationReceipt::producer_prefix_digest);
        if receipt.producer_binding_digest() != producer_binding
            || receipt.producer_prefix_digest()
                != native_observation_prefix_digest(
                    producer_binding,
                    previous_prefix,
                    &pending_prefix,
                )
            || pending_prefix.last().map(|value| value.sequence())
            != Some(receipt.through_sequence())
        {
            return Err(process_error(&lifecycle_error(
                "native observation receipt does not match the pending producer prefix",
            )));
        }
        self.pending_observations.drain(..count);
        self.acknowledged_observations = Some(receipt);
        Ok(())
    }

    fn poll_resources(&mut self, tree: ProcessTreeIdentity) -> Result<NativePoll, ProcessError> {
        if !matches!(self.phase, SessionPhase::Active | SessionPhase::Cancelling) {
            return Err(process_error(&lifecycle_error(
                "resource polling requires an active or cancelling session",
            )));
        }
        let identity = self.recovery.identity();
        if identity.root_pid() != Some(tree.root_pid())
            || identity.root_start_token() != tree.start_token()
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

    fn activated_while(
        &mut self,
        tree: ProcessTreeIdentity,
        should_continue: &mut dyn FnMut() -> bool,
    ) -> Result<(), ProcessError> {
        self.record_activation_while(tree, should_continue)
            .map_err(|error| process_error(&error))
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

pub(crate) fn process_error(error: &MacosError) -> ProcessError {
    let (code, operation) = match error.kind() {
        MacosErrorKind::InvalidInput | MacosErrorKind::LimitExceeded => {
            (ErrorCode::InvalidInput, ProcessOperation::Validate)
        }
        MacosErrorKind::UnsupportedHost => {
            (ErrorCode::Unsupported, ProcessOperation::Validate)
        }
        MacosErrorKind::DescriptorMismatch | MacosErrorKind::PreparationMismatch => {
            (ErrorCode::PlanMismatch, ProcessOperation::Validate)
        }
        MacosErrorKind::ResourceLimit => {
            (ErrorCode::ResourceLimit, ProcessOperation::Control)
        }
        MacosErrorKind::RecoveryIndeterminate => {
            (ErrorCode::Indeterminate, ProcessOperation::Reconcile)
        }
        MacosErrorKind::CleanupIncomplete => {
            (ErrorCode::Indeterminate, ProcessOperation::Reconcile)
        }
        _ => (ErrorCode::Supervisor, ProcessOperation::Wait),
    };
    let recovery = match error.recovery() {
        RecoveryAction::CorrectRequest => ProcessRecovery::CorrectRequest,
        RecoveryAction::SelectSupportedBackend => ProcessRecovery::SelectBackend,
        RecoveryAction::Reauthorize | RecoveryAction::RepairHelper => ProcessRecovery::Reauthorize,
        RecoveryAction::CancelAndReap => ProcessRecovery::CancelAndReap,
        RecoveryAction::RetryCleanup | RecoveryAction::Reconcile => {
            ProcessRecovery::ReopenAndReconcile
        }
        RecoveryAction::Quarantine => ProcessRecovery::Quarantine,
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
    let process = ProcessError::with_source(code, operation, recovery, detail, error.clone());
    if error.operation() == MacosOperation::Prepare {
        process.with_preparation_cleanup(error.preparation_cleanup().is_complete())
    } else {
        process
    }
}
