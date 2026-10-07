//! Supervised native session lifecycle.

use std::{collections::VecDeque, sync::Arc};

use peritus_network::ManagedProxy;
use peritus_process::{
    CancellationReason, NativeLaunchDescription, NativeObservationReceipt,
};
use peritus_sandbox::{
    CapabilityDomain, EnforcementObservation, ObservationDisposition, ObservationKind,
    ObservationTail,
};
use peritus_secrets::SecretDeliverySession;
use peritus_types::Sha256Digest;

use crate::{
    CleanupProgress, EnforcementLevel, HelperManifest, MacosError, MacosErrorKind,
    MacosObservation, MacosOperation, MacosRecoveryRecord, ObservationEvent, ObservationStatus,
    PreparationCleanup, RecoveryAction, RuntimeIdentity, resource_monitor::ResourceMonitor,
};

mod adapter;
mod cleanup;
mod lifecycle;
mod observation_mapping;
#[cfg(all(test, unix))]
mod tests;

pub(crate) use adapter::process_error;
use observation_mapping::{protected_handles_match, push_native_mapping};

const MAX_DIAGNOSTIC_OBSERVATIONS: usize = 4_096;
const COMMON_OBSERVATION_TAIL_LIMIT: usize = 64;

pub(crate) struct SessionResources {
    exec_status: crate::exec_status::ExecStatusOwner,
    proxy: Option<ManagedProxy>,
    secrets: SecretDeliverySession,
    preparation_continues: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl SessionResources {
    #[cfg(all(test, unix))]
    pub(crate) fn new(
        exec_status: crate::exec_status::ExecStatusOwner,
        proxy: Option<ManagedProxy>,
        secrets: SecretDeliverySession,
    ) -> Self {
        Self::new_cancellable(exec_status, proxy, secrets, Arc::new(|| true))
    }

    pub(crate) fn new_cancellable(
        exec_status: crate::exec_status::ExecStatusOwner,
        proxy: Option<ManagedProxy>,
        secrets: SecretDeliverySession,
        preparation_continues: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self { exec_status, proxy, secrets, preparation_continues }
    }

    fn cleanup_after_preparation_failure(mut self, original: MacosError) -> MacosError {
        let support = self.exec_status.finish().err();
        let secrets = self.secrets.release().err();
        let proxy = self.proxy.take().and_then(|proxy| proxy.shutdown().err());
        let original_cleanup = original.preparation_cleanup();
        let cleanup = PreparationCleanup::new(
            original_cleanup.support_join() || support.is_some(),
            original_cleanup.proxy_shutdown() || proxy.is_some(),
            original_cleanup.secret_release() || secrets.is_some(),
        );
        if cleanup.is_complete() {
            return original;
        }
        let mut error = MacosError::new(
            MacosErrorKind::CleanupIncomplete,
            MacosOperation::Prepare,
            RecoveryAction::RetryCleanup,
            format!(
                "native preparation failed with {}; one or more started resource families require reconciliation",
                original.code()
            ),
        )
        .with_cleanup(cleanup);
        if let Some(source) = secrets.as_ref().map(crate::error::secret_source) {
            error = error.with_source(source);
        } else if let Some(source) = proxy.as_ref().map(crate::error::network_source) {
            error = error.with_source(source);
        } else if let Some(source) = support.as_ref().and_then(MacosError::cause) {
            error = error.with_source(source);
        }
        error
    }
}

/// macOS backend lifecycle phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SessionPhase {
    /// Manifest and profile are prepared without a target effect.
    Prepared,
    /// C2 verified the helper activation handshake and process tree.
    Active,
    /// C2 accepted a cancellation request.
    Cancelling,
    /// Root/helper termination was observed.
    Terminated,
    /// Every backend-owned resource was released.
    Released,
}

/// Stable reason retained for an observed termination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminationReason {
    /// Target exited with an ordinary numeric status.
    TargetExit(i32),
    /// Target or helper was terminated by a signal/platform status.
    Signalled,
    /// No trustworthy operating-system status was available.
    Unavailable,
}

/// Teardown evidence returned by an idempotent release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReleaseReport {
    cleanup: CleanupProgress,
    already_released: bool,
}

impl ReleaseReport {
    /// Returns exact cleanup progress.
    #[must_use]
    pub const fn cleanup(self) -> CleanupProgress {
        self.cleanup
    }

    /// Reports whether release was already complete.
    #[must_use]
    pub const fn already_released(self) -> bool {
        self.already_released
    }
}

/// Prepared native session retained by the C2 supervisor until release.
pub struct MacosSession {
    launch: NativeLaunchDescription,
    manifest: HelperManifest,
    phase: SessionPhase,
    termination: Option<TerminationReason>,
    cancellation: Option<CancellationReason>,
    observations: ObservationTail<EnforcementObservation>,
    pending_observations: VecDeque<EnforcementObservation>,
    acknowledged_observations: Option<NativeObservationReceipt>,
    next_observation_sequence: u64,
    native_observations: ObservationTail<MacosObservation>,
    next_native_observation_sequence: u64,
    recovery: MacosRecoveryRecord,
    cleanup: CleanupProgress,
    resource_monitor: ResourceMonitor,
    exec_status: crate::exec_status::ExecStatusOwner,
    proxy: Option<ManagedProxy>,
    proxy_cleanup_failed: bool,
    secrets: SecretDeliverySession,
}

impl core::fmt::Debug for MacosSession {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("MacosSession")
            .field("phase", &self.phase)
            .field("manifest_digest", &self.manifest.digest())
            .field("termination", &self.termination)
            .field("cancellation", &self.cancellation)
            .field("observation_count", &self.observations.as_slice().len())
            .field("cleanup", &self.cleanup)
            .finish_non_exhaustive()
    }
}

impl MacosSession {
    #[allow(clippy::too_many_lines, reason = "preparation records every mapped native domain")]
    pub(crate) fn new(
        launch: NativeLaunchDescription,
        manifest: HelperManifest,
        helper_digest: Sha256Digest,
        proxy_routing_digest: Option<Sha256Digest>,
        observation_limit: usize,
        resources: SessionResources,
    ) -> Result<Self, MacosError> {
        let diagnostic_limit = observation_limit.min(MAX_DIAGNOSTIC_OBSERVATIONS);
        if manifest.proxy().is_some() != resources.proxy.is_some()
            || manifest.secrets().len() != resources.secrets.artifacts().len()
            || !protected_handles_match(&launch, &manifest)
        {
            let error = crate::error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "native protected owners differ from helper manifest bindings",
            );
            return Err(resources.cleanup_after_preparation_failure(error));
        }
        let cleanup =
            CleanupProgress::prepared(manifest.proxy().is_some(), !manifest.secrets().is_empty());
        let preparation_continues = Arc::clone(&resources.preparation_continues);
        let resource_monitor = match ResourceMonitor::new_cancellable(
            manifest.working_directory(),
            manifest.resources(),
            || preparation_continues(),
        ) {
            Ok(monitor) => monitor,
            Err(error) => return Err(resources.cleanup_after_preparation_failure(error)),
        };
        let identity = RuntimeIdentity::new(
            manifest.process_id(),
            manifest.preparation_digest(),
            manifest.profile_digest(),
            helper_digest,
            proxy_routing_digest,
            crate::secret_binding_digest(manifest.secrets()),
            None,
            None,
        );
        let recovery = match MacosRecoveryRecord::new(identity, false, cleanup) {
            Ok(recovery) => recovery,
            Err(error) => return Err(resources.cleanup_after_preparation_failure(error)),
        };
        let SessionResources {
            exec_status,
            proxy,
            secrets,
            preparation_continues: _,
        } = resources;
        let common = EnforcementObservation::new(
            1,
            manifest.plan_digest(),
            manifest.descriptor_digest(),
            ObservationKind::Prepared,
            None,
            ObservationDisposition::Completed,
        );
        let native = MacosObservation::new(
            1,
            manifest.plan_digest(),
            manifest.descriptor_digest(),
            manifest.preparation_digest(),
            manifest.profile_digest(),
            ObservationEvent::Prepared,
            None,
            None,
            None,
            ObservationStatus::Completed,
        );
        let mut observations = ObservationTail::new(COMMON_OBSERVATION_TAIL_LIMIT);
        observations.push(common);
        let pending_observations = VecDeque::from([common]);
        let mut native_observations = ObservationTail::new(diagnostic_limit);
        native_observations.push(native);
        let mut next_native_observation_sequence = 2_u64;
        for (domain, enforcement) in [
            (CapabilityDomain::Filesystem, EnforcementLevel::Hard),
            (CapabilityDomain::Process, EnforcementLevel::Supervisor),
            (CapabilityDomain::Environment, EnforcementLevel::Supervisor),
            (CapabilityDomain::Network, EnforcementLevel::Hard),
            (CapabilityDomain::Terminal, EnforcementLevel::Supervisor),
        ] {
            push_native_mapping(
                &mut native_observations,
                &mut next_native_observation_sequence,
                &manifest,
                ObservationEvent::ControlMapped,
                domain,
                None,
                enforcement,
            );
        }
        for control in manifest.resources().controls() {
            push_native_mapping(
                &mut native_observations,
                &mut next_native_observation_sequence,
                &manifest,
                ObservationEvent::ResourceMapped,
                CapabilityDomain::Resource,
                Some(control.kind()),
                control.level(),
            );
        }
        if manifest.proxy().is_some() {
            push_native_mapping(
                &mut native_observations,
                &mut next_native_observation_sequence,
                &manifest,
                ObservationEvent::ProxyMapped,
                CapabilityDomain::Network,
                None,
                EnforcementLevel::Supervisor,
            );
        }
        if !manifest.secrets().is_empty() {
            push_native_mapping(
                &mut native_observations,
                &mut next_native_observation_sequence,
                &manifest,
                ObservationEvent::ControlMapped,
                CapabilityDomain::Secret,
                None,
                EnforcementLevel::Supervisor,
            );
        }
        Ok(Self {
            launch,
            manifest,
            phase: SessionPhase::Prepared,
            termination: None,
            cancellation: None,
            observations,
            pending_observations,
            acknowledged_observations: None,
            next_observation_sequence: 2,
            native_observations,
            next_native_observation_sequence,
            recovery,
            cleanup,
            resource_monitor,
            exec_status,
            proxy,
            proxy_cleanup_failed: false,
            secrets,
        })
    }

    /// Returns the current lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> SessionPhase {
        self.phase
    }

    /// Returns the exact helper manifest.
    #[must_use]
    pub const fn manifest(&self) -> &HelperManifest {
        &self.manifest
    }

    /// Returns rich preparation-bound macOS observations.
    #[must_use]
    pub fn native_observations(&self) -> &[MacosObservation] {
        self.native_observations.as_slice()
    }
    /// Returns rich macOS observations omitted before the retained diagnostic tail.
    #[must_use]
    pub const fn native_observations_dropped(&self) -> u64 {
        self.native_observations.dropped()
    }

    /// Returns the latest durable recovery record.
    #[must_use]
    pub const fn recovery_record(&self) -> &MacosRecoveryRecord {
        &self.recovery
    }

    /// Returns the observed termination category.
    #[must_use]
    pub const fn termination(&self) -> Option<TerminationReason> {
        self.termination
    }
}
