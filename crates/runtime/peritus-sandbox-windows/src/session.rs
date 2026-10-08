//! C2-owned Windows native session lifecycle and teardown.

use std::collections::VecDeque;

use peritus_network::ManagedProxy;
use peritus_process::{
    CancellationReason, NATIVE_OBSERVATION_PAGE_RECORDS, NativeLaunchDescription,
    NativeObservationPage, NativeObservationReceipt, NativeObservationTransport,
    NativePlatform, NativeRecoveryPhase, NativeSandboxSession, NativeSessionRecovery,
    OsExitObservation, ProcessError, ProcessTreeIdentity,
    native_observation_prefix_digest, native_observation_producer_binding,
};
use peritus_sandbox::{EnforcementObservation, ObservationDisposition, ObservationTail};
use peritus_secrets::SecretDeliverySession;

use crate::{
    AclTransaction, CleanupState, ObservationBinding, ObservationStatus, ReleaseProgress,
    RecoveryCleanup, ReleaseReport, ResourceControlPlan, RuntimeIdentity, WindowsError, WindowsErrorKind,
    WindowsLaunchDescription, WindowsObservation, WindowsOperation, WindowsPhase, WindowsRecovery,
    WindowsRecoveryRecord,
    network_filter::NetworkFilterOwner,
    observation::{WindowsCapability, observation_error, transition_allowed},
    recovery::RecoveryCleanupDimension,
};

mod teardown;

const RICH_OBSERVATION_LIMIT: usize = 64;
const COMMON_OBSERVATION_TAIL_LIMIT: usize = 64;

/// Prepared Windows session retained by C2 until release.
#[derive(Debug)]
pub struct WindowsSession {
    native_launch: NativeLaunchDescription,
    windows_launch: WindowsLaunchDescription,
    acl: AclTransaction,
    phase: WindowsPhase,
    binding: ObservationBinding,
    observations: ObservationTail<EnforcementObservation>,
    pending_observations: VecDeque<EnforcementObservation>,
    acknowledged_observations: Option<NativeObservationReceipt>,
    next_observation_sequence: u64,
    windows_observations: ObservationTail<WindowsObservation>,
    next_windows_observation_sequence: u64,
    resources: ResourceControlPlan,
    recovery: WindowsRecoveryRecord,
    proxy: Option<ManagedProxy>,
    proxy_cleanup: CleanupState,
    filter: NetworkFilterOwner,
    filter_cleanup: CleanupState,
    secrets: Option<SecretDeliverySession>,
    secret_cleanup: CleanupState,
    job_cleanup: CleanupState,
    helper_cleanup: CleanupState,
    secret_file_cleanup: CleanupState,
    handle_cleanup: CleanupState,
    release: Option<ReleaseReport>,
}

impl WindowsSession {
    #[allow(clippy::too_many_arguments, reason = "complete prepared native-session identity")]
    pub(crate) fn new(
        native_launch: NativeLaunchDescription,
        windows_launch: WindowsLaunchDescription,
        acl: AclTransaction,
        resources: ResourceControlPlan,
        binding: ObservationBinding,
        runtime_identity: RuntimeIdentity,
        proxy: Option<ManagedProxy>,
        filter: NetworkFilterOwner,
        secrets: Option<SecretDeliverySession>,
    ) -> Self {
        let prepared = binding.common(1, WindowsPhase::Prepared, ObservationDisposition::Completed);
        let mut observations = ObservationTail::new(COMMON_OBSERVATION_TAIL_LIMIT);
        observations.push(prepared);
        let pending_observations = VecDeque::from([prepared]);
        let mut windows_observations = ObservationTail::new(RICH_OBSERVATION_LIMIT);
        let mut next_windows_observation_sequence = 1_u64;
        for capability in [
            WindowsCapability::RestrictedToken,
            WindowsCapability::LowIntegrity,
            WindowsCapability::AppContainer,
            WindowsCapability::JobObject,
            WindowsCapability::Acl,
            WindowsCapability::PathResolution,
            WindowsCapability::HandleList,
            WindowsCapability::ConPty,
            WindowsCapability::Network,
            WindowsCapability::SecretHandles,
        ] {
            windows_observations.push(WindowsObservation::new(
                next_windows_observation_sequence,
                binding,
                WindowsPhase::Prepared,
                Some(capability),
                None,
                None,
                ObservationStatus::Verified,
            ));
            next_windows_observation_sequence =
                next_windows_observation_sequence.saturating_add(1);
        }
        for control in resources.controls() {
            if !control.is_selected() {
                continue;
            }
            windows_observations.push(WindowsObservation::new(
                next_windows_observation_sequence,
                binding,
                WindowsPhase::Prepared,
                None,
                Some(control.kind()),
                Some(control.level()),
                ObservationStatus::Installed,
            ));
            next_windows_observation_sequence =
                next_windows_observation_sequence.saturating_add(1);
        }
        let proxy_cleanup =
            if proxy.is_some() { CleanupState::Pending } else { CleanupState::Complete };
        let filter_cleanup =
            if filter.is_managed() { CleanupState::Pending } else { CleanupState::Complete };
        let secret_cleanup =
            if secrets.is_some() { CleanupState::Pending } else { CleanupState::Complete };
        #[cfg(target_os = "windows")]
        let containment_required =
            native_launch.retains_windows_job(runtime_identity.job_identity());
        #[cfg(not(target_os = "windows"))]
        let containment_required = false;
        let secret_files_required = windows_launch
            .manifest()
            .secret_handles()
            .iter()
            .any(|secret| matches!(secret.destination(), crate::SecretHandleDestination::File(_)));
        let cleanup = RecoveryCleanup::prepared(
            containment_required,
            !acl.restored(),
            secret_files_required,
            secrets.is_some(),
            !native_launch.protected_handles().is_empty(),
            proxy.is_some(),
            filter.is_managed(),
        );
        let job_cleanup = cleanup_state(cleanup.job_closed());
        let helper_cleanup = cleanup_state(cleanup.helper_reaped());
        let secret_file_cleanup = cleanup_state(cleanup.secret_files_removed());
        let handle_cleanup = cleanup_state(cleanup.handles_closed());
        let recovery = WindowsRecoveryRecord::prepared_owned(
            runtime_identity,
            containment_required,
            cleanup,
            acl.transaction_digest(),
            acl.receipt(),
            acl.owner_operation_digest(),
            acl.service_owner_digest(),
        );
        Self {
            native_launch,
            windows_launch,
            acl,
            phase: WindowsPhase::Prepared,
            binding,
            observations,
            pending_observations,
            acknowledged_observations: None,
            next_observation_sequence: 2,
            windows_observations,
            next_windows_observation_sequence,
            resources,
            recovery,
            proxy,
            proxy_cleanup,
            filter,
            filter_cleanup,
            secrets,
            secret_cleanup,
            job_cleanup,
            helper_cleanup,
            secret_file_cleanup,
            handle_cleanup,
            release: None,
        }
    }

    /// Returns backend-local launch details.
    #[must_use]
    pub const fn windows_launch_description(&self) -> &WindowsLaunchDescription {
        &self.windows_launch
    }

    /// Returns rich Windows observations.
    #[must_use]
    pub fn windows_observations(&self) -> &[WindowsObservation] {
        self.windows_observations.as_slice()
    }
    /// Returns rich Windows observations omitted before the retained diagnostic tail.
    #[must_use]
    pub const fn windows_observations_dropped(&self) -> u64 {
        self.windows_observations.dropped()
    }

    /// Returns dimension-specific resource enforcement.
    #[must_use]
    pub const fn resource_controls(&self) -> ResourceControlPlan {
        self.resources
    }

    /// Returns current durable recovery evidence.
    #[must_use]
    pub const fn recovery_record(&self) -> &WindowsRecoveryRecord {
        &self.recovery
    }

    /// Returns final release evidence when release completed.
    #[must_use]
    pub const fn release_report(&self) -> Option<ReleaseReport> {
        self.release
    }

    /// Returns partial cleanup evidence, including failed retryable dimensions.
    #[must_use]
    pub const fn release_progress(&self) -> ReleaseProgress {
        ReleaseProgress::new(
            self.acl.cleanup_state(),
            self.job_cleanup,
            self.helper_cleanup,
            self.secret_file_cleanup,
            self.secret_cleanup,
            self.handle_cleanup,
            self.proxy_cleanup,
            self.filter_cleanup,
        )
    }

    fn transition(
        &mut self,
        next: WindowsPhase,
        disposition: ObservationDisposition,
    ) -> Result<(), WindowsError> {
        if !transition_allowed(self.phase, next) {
            return Err(observation_error("Windows lifecycle transition is out of order"));
        }
        self.push_common(next, disposition)?;
        self.push_rich(next, ObservationStatus::Verified);
        self.phase = next;
        Ok(())
    }

    fn push_common(
        &mut self,
        next: WindowsPhase,
        disposition: ObservationDisposition,
    ) -> Result<(), WindowsError> {
        let sequence = self.next_observation_sequence;
        self.next_observation_sequence = sequence
            .checked_add(1)
            .ok_or_else(|| observation_error("common observation sequence overflowed"))?;
        let observation = self.binding.common(sequence, next, disposition);
        self.pending_observations.push_back(observation);
        self.observations.push(observation);
        Ok(())
    }

    fn push_rich(&mut self, phase: WindowsPhase, status: ObservationStatus) {
        let Some(next) = self.next_windows_observation_sequence.checked_add(1) else {
            return;
        };
        self.windows_observations.push(WindowsObservation::new(
            self.next_windows_observation_sequence,
            self.binding,
            phase,
            None,
            None,
            None,
            status,
        ));
        self.next_windows_observation_sequence = next;
    }
}

impl NativeSandboxSession for WindowsSession {
    fn launch_description(&self) -> &NativeLaunchDescription {
        &self.native_launch
    }

    fn recovery_snapshot(&self) -> Result<Option<NativeSessionRecovery>, ProcessError> {
        let phase = if self.recovery.cleanup_complete() {
            NativeRecoveryPhase::Released
        } else {
            match self.recovery.phase() {
                WindowsPhase::Prepared => NativeRecoveryPhase::Prepared,
                WindowsPhase::Activated => NativeRecoveryPhase::Active,
                WindowsPhase::CancelRequested => NativeRecoveryPhase::Cancelling,
                WindowsPhase::Terminated => NativeRecoveryPhase::Terminated,
                WindowsPhase::Released => NativeRecoveryPhase::Released,
            }
        };
        let bytes = self.recovery.canonical_bytes();
        let mut record = Vec::new();
        record.try_reserve_exact(bytes.len()).map_err(|_| {
            process_error(&WindowsError::new(
                WindowsErrorKind::RecoveryIndeterminate,
                WindowsOperation::Recover,
                WindowsRecovery::Quarantine,
                "Windows recovery record cannot be transferred",
            ))
        })?;
        record.extend_from_slice(bytes);
        #[cfg(target_os = "windows")]
        let custody_complete = self.recovery.custody_complete()
            && self
                .native_launch
                .retains_windows_job(self.recovery.identity().job_identity());
        #[cfg(not(target_os = "windows"))]
        let custody_complete = self.recovery.custody_complete();
        let snapshot = NativeSessionRecovery::new(
            NativePlatform::Windows,
            self.recovery.identity().process_id(),
            phase,
            self.recovery.tree_identity(),
            self.recovery.owner_operation_digest(),
            self.recovery.service_owner_digest(),
            custody_complete,
            record,
        )?;
        let snapshot = match self.recovery.containment_identity() {
            Some(containment) => snapshot.with_windows_containment(containment.clone())?,
            None => snapshot,
        };
        Ok(Some(snapshot))
    }

    fn spawned(&mut self, tree: ProcessTreeIdentity) -> Result<(), ProcessError> {
        self.recovery.spawned(tree).map_err(|error| process_error(&error))
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
            return Err(process_error(&observation_error(
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
            .ok_or_else(|| process_error(&observation_error(
                "native observation receipt is outside the pending prefix",
            )))?;
        let pending_prefix = self
            .pending_observations
            .iter()
            .copied()
            .take(count)
            .collect::<Vec<_>>();
        let producer_binding = native_observation_producer_binding(
            self.native_launch.manifest_digest(),
            self.native_launch.preparation_digest(),
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
            return Err(process_error(&observation_error(
                "native observation receipt does not match the pending producer prefix",
            )));
        }
        self.pending_observations.drain(..count);
        self.acknowledged_observations = Some(receipt);
        Ok(())
    }

    fn activated(&mut self, tree: ProcessTreeIdentity) -> Result<(), ProcessError> {
        if !tree.complete_containment() || self.recovery.tree_identity() != Some(tree) {
            return Err(process_error(&WindowsError::new(
                WindowsErrorKind::Job,
                WindowsOperation::Activate,
                WindowsRecovery::CancelAndReap,
                "C2 helper/target tree differs from the retained Windows birth identity",
            )));
        }
        #[cfg(target_os = "windows")]
        {
            let secret_files = self
                .native_launch
                .windows_secret_files()?
                .ok_or_else(|| {
                    process_error(&WindowsError::new(
                        WindowsErrorKind::Secret,
                        WindowsOperation::Activate,
                        WindowsRecovery::CancelAndReap,
                        "C2 retained no exact private secret-file custody record",
                    ))
                })?;
            let secret_files = secret_file_recovery(&self.windows_launch, secret_files)?;
            self.recovery
                .retain_secret_files(secret_files)
                .map_err(|error| process_error(&error))?;
            let containment = self
                .native_launch
                .windows_containment_identity()?
                .ok_or_else(|| {
                    process_error(&WindowsError::new(
                        WindowsErrorKind::Job,
                        WindowsOperation::Activate,
                        WindowsRecovery::CancelAndReap,
                        "C2 retained no exact Job Object and target adoption identity",
                    ))
                })?;
            self.recovery
                .adopted(containment)
                .map_err(|error| process_error(&error))?;
        }
        self.transition(WindowsPhase::Activated, ObservationDisposition::Completed)
            .map_err(|error| process_error(&error))?;
        self.recovery
            .advance_phase(WindowsPhase::Activated)
            .map_err(|error| process_error(&error))
    }

    fn cancellation_requested(&mut self, _reason: CancellationReason) -> Result<(), ProcessError> {
        if self.phase == WindowsPhase::CancelRequested {
            return Ok(());
        }
        self.transition(WindowsPhase::CancelRequested, ObservationDisposition::Accepted)
            .map_err(|error| process_error(&error))?;
        self.recovery
            .advance_phase(WindowsPhase::CancelRequested)
            .map_err(|error| process_error(&error))
    }

    fn terminated(&mut self, _exit: &OsExitObservation) -> Result<(), ProcessError> {
        self.transition(WindowsPhase::Terminated, ObservationDisposition::Completed)
            .map_err(|error| process_error(&error))?;
        let result = self
            .recovery
            .advance_phase_with_cleanup(WindowsPhase::Terminated, RecoveryCleanupDimension::Helper)
            .map_err(|error| process_error(&error));
        self.helper_cleanup = if result.is_ok() {
            CleanupState::Complete
        } else {
            CleanupState::RetryRequired
        };
        result
    }

    fn release(&mut self) -> Result<(), ProcessError> {
        if self.release.is_some() {
            return Ok(());
        }
        let normal_release = self.phase == WindowsPhase::Terminated;
        let report = self.release_owned_resources()?;
        if normal_release {
            self.transition(WindowsPhase::Released, ObservationDisposition::Completed)
                .map_err(|error| process_error(&error))?;
            self.recovery
                .advance_phase(WindowsPhase::Released)
                .map_err(|error| process_error(&error))?;
        } else {
            if self.phase == WindowsPhase::Prepared {
                self.push_common(WindowsPhase::Released, ObservationDisposition::Completed)
                    .map_err(|error| process_error(&error))?;
                self.phase = WindowsPhase::Released;
            }
            self.record_abort_cleanup();
            self.recovery
                .record_cleanup(true, true, true)
                .map_err(|error| process_error(&error))?;
        }
        self.release = Some(report);
        Ok(())
    }
}

impl WindowsSession {
    fn record_abort_cleanup(&mut self) {
        self.push_rich(self.phase, ObservationStatus::Verified);
    }
}

const fn cleanup_state(complete: bool) -> CleanupState {
    if complete { CleanupState::Complete } else { CleanupState::Pending }
}

#[cfg(target_os = "windows")]
pub(super) fn secret_file_recovery(
    launch: &WindowsLaunchDescription,
    mut identities: Vec<peritus_process::NativeWindowsSecretFileIdentity>,
) -> Result<Vec<crate::recovery::SecretFileRecovery>, ProcessError> {
    let mut files = Vec::new();
    for descriptor in launch.manifest().secret_handles() {
        let crate::SecretHandleDestination::File(path) = descriptor.destination() else {
            continue;
        };
        let native = crate::WindowsPath::from_sandbox(
            launch.manifest().working_directory(),
            path,
        )
        .map_err(|error| process_error(&error))?;
        let position = identities
            .iter()
            .position(|identity| identity.binding().path_digest() == native.digest())
            .ok_or_else(|| {
                process_error(&WindowsError::new(
                    WindowsErrorKind::RecoveryIndeterminate,
                    WindowsOperation::Recover,
                    WindowsRecovery::Quarantine,
                    "helper secret-file custody omitted a manifest path",
                ))
            })?;
        let identity = identities.remove(position);
        if descriptor.payload_len() != Some(identity.binding().payload_len()) {
            return Err(process_error(&WindowsError::new(
                WindowsErrorKind::RecoveryIndeterminate,
                WindowsOperation::Recover,
                WindowsRecovery::Quarantine,
                "helper secret-file custody changed a manifest payload length",
            )));
        }
        files.push(
            crate::recovery::SecretFileRecovery::new(native, identity)
                .map_err(|error| process_error(&error))?,
        );
    }
    if !identities.is_empty() {
        return Err(process_error(&WindowsError::new(
            WindowsErrorKind::RecoveryIndeterminate,
            WindowsOperation::Recover,
            WindowsRecovery::Quarantine,
            "helper secret-file custody contains an unknown path",
        )));
    }
    Ok(files)
}

pub(crate) fn process_error(error: &WindowsError) -> ProcessError {
    use peritus_process::{ErrorCode, ProcessOperation, RecoveryClass};
    let (code, operation, recovery, detail) = if !error.preparation_cleanup().is_complete() {
        (
            ErrorCode::Indeterminate,
            ProcessOperation::Reconcile,
            RecoveryClass::ReopenAndReconcile,
            "Windows preparation cleanup requires exact reconciliation",
        )
    } else {
        match error.kind() {
        WindowsErrorKind::InvalidPlan | WindowsErrorKind::Path => (
            ErrorCode::InvalidInput,
            ProcessOperation::Validate,
            RecoveryClass::CorrectRequest,
            "Windows native plan or path cannot be represented exactly",
        ),
        WindowsErrorKind::UnsupportedHost | WindowsErrorKind::ProbeFailed => (
            ErrorCode::Unsupported,
            ProcessOperation::Validate,
            RecoveryClass::SelectBackend,
            "Windows host lacks a required native enforcement control",
        ),
        WindowsErrorKind::DescriptorMismatch | WindowsErrorKind::PreparationMismatch => (
            ErrorCode::PlanMismatch,
            ProcessOperation::Validate,
            RecoveryClass::SelectBackend,
            "Windows native identity differs from C2 admission",
        ),
        WindowsErrorKind::RecoveryIndeterminate => (
            ErrorCode::Indeterminate,
            ProcessOperation::Reconcile,
            RecoveryClass::Quarantine,
            "Windows native ownership or teardown is indeterminate",
        ),
        WindowsErrorKind::Resource => (
            ErrorCode::ResourceLimit,
            ProcessOperation::Spawn,
            RecoveryClass::CancelAndReap,
            "Windows hard resource control could not be installed",
        ),
        WindowsErrorKind::SandboxDenied
        | WindowsErrorKind::HelperProtocol
        | WindowsErrorKind::Acl
        | WindowsErrorKind::Token
        | WindowsErrorKind::AppContainer
        | WindowsErrorKind::Job
        | WindowsErrorKind::Handle
        | WindowsErrorKind::Terminal
        | WindowsErrorKind::Network
        | WindowsErrorKind::Secret
        | WindowsErrorKind::Observation
        | WindowsErrorKind::Io => (
            ErrorCode::Supervisor,
            ProcessOperation::Spawn,
            RecoveryClass::CancelAndReap,
            "Windows native preparation or lifecycle operation failed",
        ),
        }
    };
    ProcessError::with_source(code, operation, recovery, detail, error.clone())
}
