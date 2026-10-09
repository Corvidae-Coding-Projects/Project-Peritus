//! C2-owned Windows native session lifecycle and teardown.

use peritus_network::ManagedProxy;
use peritus_process::{
    CancellationReason, NativeLaunchDescription, NativeSandboxSession, OsExitObservation,
    ProcessError, ProcessTreeIdentity,
};
use peritus_sandbox::{EnforcementObservation, ObservationDisposition};
use peritus_secrets::SecretDeliverySession;

use crate::{
    AclTransaction, CleanupState, ObservationBinding, ObservationStatus, ReleaseProgress,
    ReleaseReport, ResourceControlPlan, RuntimeIdentity, WindowsError, WindowsErrorKind,
    WindowsLaunchDescription, WindowsObservation, WindowsOperation, WindowsPhase, WindowsRecovery,
    WindowsRecoveryRecord,
    network_filter::NetworkFilterOwner,
    observation::{WindowsCapability, observation_error, transition_allowed},
};

mod teardown;

const RICH_OBSERVATION_LIMIT: usize = 64;

/// Prepared Windows session retained by C2 until release.
#[derive(Debug)]
pub struct WindowsSession {
    native_launch: NativeLaunchDescription,
    windows_launch: WindowsLaunchDescription,
    acl: AclTransaction,
    phase: WindowsPhase,
    binding: ObservationBinding,
    observations: Vec<EnforcementObservation>,
    windows_observations: Vec<WindowsObservation>,
    resources: ResourceControlPlan,
    recovery: WindowsRecoveryRecord,
    proxy: Option<ManagedProxy>,
    proxy_cleanup: CleanupState,
    filter: NetworkFilterOwner,
    secrets: Option<SecretDeliverySession>,
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
        let observations =
            vec![binding.common(1, WindowsPhase::Prepared, ObservationDisposition::Completed)];
        let mut windows_observations = Vec::new();
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
            let sequence = u64::try_from(windows_observations.len() + 1).unwrap_or(u64::MAX);
            windows_observations.push(WindowsObservation::new(
                sequence,
                binding,
                WindowsPhase::Prepared,
                Some(capability),
                None,
                None,
                ObservationStatus::Verified,
            ));
        }
        for control in resources.controls() {
            let sequence = u64::try_from(windows_observations.len() + 1).unwrap_or(u64::MAX);
            windows_observations.push(WindowsObservation::new(
                sequence,
                binding,
                WindowsPhase::Prepared,
                None,
                Some(control.kind()),
                Some(control.level()),
                ObservationStatus::Installed,
            ));
        }
        let proxy_cleanup =
            if proxy.is_some() { CleanupState::Pending } else { CleanupState::Complete };
        Self {
            native_launch,
            windows_launch,
            acl,
            phase: WindowsPhase::Prepared,
            binding,
            observations,
            windows_observations,
            resources,
            recovery: WindowsRecoveryRecord::prepared(runtime_identity),
            proxy,
            proxy_cleanup,
            filter,
            secrets,
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
        &self.windows_observations
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
        ReleaseProgress::new(self.acl.cleanup_state(), self.proxy_cleanup)
    }

    fn transition(
        &mut self,
        next: WindowsPhase,
        disposition: ObservationDisposition,
    ) -> Result<(), WindowsError> {
        if !transition_allowed(self.phase, next) {
            return Err(observation_error("Windows lifecycle transition is out of order"));
        }
        if self.windows_observations.len() >= RICH_OBSERVATION_LIMIT {
            return Err(observation_error("Windows observation bound is exhausted"));
        }
        let common_sequence = u64::try_from(self.observations.len() + 1)
            .map_err(|_| observation_error("common observation sequence overflowed"))?;
        self.observations.push(self.binding.common(common_sequence, next, disposition));
        let rich_sequence = u64::try_from(self.windows_observations.len() + 1)
            .map_err(|_| observation_error("Windows observation sequence overflowed"))?;
        self.windows_observations.push(WindowsObservation::new(
            rich_sequence,
            self.binding,
            next,
            None,
            None,
            None,
            ObservationStatus::Verified,
        ));
        self.phase = next;
        Ok(())
    }
}

impl NativeSandboxSession for WindowsSession {
    fn launch_description(&self) -> &NativeLaunchDescription {
        &self.native_launch
    }

    fn observations(&self) -> &[EnforcementObservation] {
        &self.observations
    }

    fn activated(&mut self, tree: ProcessTreeIdentity) -> Result<(), ProcessError> {
        if !tree.complete_containment() {
            return Err(process_error(&WindowsError::new(
                WindowsErrorKind::Job,
                WindowsOperation::Activate,
                WindowsRecovery::CancelAndReap,
                "C2 did not establish complete helper/target tree containment",
            )));
        }
        self.transition(WindowsPhase::Activated, ObservationDisposition::Completed)
            .map_err(|error| process_error(&error))?;
        self.recovery
            .advance(WindowsPhase::Activated, false, false, false)
            .map_err(|error| process_error(&error))
    }

    fn cancellation_requested(&mut self, _reason: CancellationReason) -> Result<(), ProcessError> {
        if self.phase == WindowsPhase::CancelRequested {
            return Ok(());
        }
        self.transition(WindowsPhase::CancelRequested, ObservationDisposition::Accepted)
            .map_err(|error| process_error(&error))?;
        self.recovery
            .advance(WindowsPhase::CancelRequested, false, false, false)
            .map_err(|error| process_error(&error))
    }

    fn terminated(&mut self, _exit: &OsExitObservation) -> Result<(), ProcessError> {
        self.transition(WindowsPhase::Terminated, ObservationDisposition::Completed)
            .map_err(|error| process_error(&error))?;
        self.recovery
            .advance(WindowsPhase::Terminated, false, false, true)
            .map_err(|error| process_error(&error))
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
                .advance(WindowsPhase::Released, true, true, true)
                .map_err(|error| process_error(&error))?;
        } else {
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
        if self.windows_observations.len() >= RICH_OBSERVATION_LIMIT {
            return;
        }
        let sequence = u64::try_from(self.windows_observations.len() + 1).unwrap_or(u64::MAX);
        self.windows_observations.push(WindowsObservation::new(
            sequence,
            self.binding,
            self.phase,
            None,
            None,
            None,
            ObservationStatus::Incomplete,
        ));
    }
}

pub(crate) const fn process_error(error: &WindowsError) -> ProcessError {
    use peritus_process::{ErrorCode, ProcessOperation, RecoveryClass};
    let (code, operation, recovery, detail) = match error.kind() {
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
    };
    ProcessError::new(code, operation, recovery, detail)
}
