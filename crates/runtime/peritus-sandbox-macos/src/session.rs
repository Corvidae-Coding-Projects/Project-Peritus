//! Supervised native session lifecycle.

use peritus_network::ManagedProxy;
use peritus_process::{CancellationReason, NativeLaunchDescription};
use peritus_sandbox::{
    CapabilityDomain, EnforcementObservation, ObservationDisposition, ObservationKind,
};
use peritus_secrets::SecretDeliverySession;
use peritus_types::Sha256Digest;

use crate::{
    CleanupProgress, EnforcementLevel, HelperManifest, MacosError, MacosErrorKind,
    MacosObservation, MacosOperation, MacosRecoveryRecord, ObservationEvent, ObservationStatus,
    RuntimeIdentity, resource_monitor::ResourceMonitor,
};

mod adapter;
mod cleanup;
mod lifecycle;
mod observation_mapping;
#[cfg(test)]
mod tests;

pub(crate) use adapter::process_error;
use observation_mapping::{protected_handles_match, push_native_mapping};

const MAX_OBSERVATIONS: usize = 4_096;

pub(crate) struct SessionResources {
    exec_status: crate::exec_status::ExecStatusOwner,
    proxy: Option<ManagedProxy>,
    secrets: SecretDeliverySession,
}

impl SessionResources {
    pub(crate) const fn new(
        exec_status: crate::exec_status::ExecStatusOwner,
        proxy: Option<ManagedProxy>,
        secrets: SecretDeliverySession,
    ) -> Self {
        Self { exec_status, proxy, secrets }
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
    observations: Vec<EnforcementObservation>,
    native_observations: Vec<MacosObservation>,
    recovery: MacosRecoveryRecord,
    cleanup: CleanupProgress,
    observation_limit: usize,
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
            .field("observation_count", &self.observations.len())
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
        let SessionResources { exec_status, proxy, secrets } = resources;
        let observation_limit = observation_limit.min(MAX_OBSERVATIONS);
        if observation_limit < 5 {
            return Err(crate::error::limited(
                MacosOperation::Prepare,
                "observation limit cannot retain native lifecycle",
            ));
        }
        if manifest.proxy().is_some() != proxy.is_some()
            || manifest.secrets().len() != secrets.artifacts().len()
            || !protected_handles_match(&launch, &manifest)
        {
            return Err(crate::error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "native protected owners differ from helper manifest bindings",
            ));
        }
        let cleanup =
            CleanupProgress::prepared(manifest.proxy().is_some(), !manifest.secrets().is_empty());
        let resource_monitor = ResourceMonitor::new(manifest.working_directory())?;
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
        let recovery = MacosRecoveryRecord::new(identity, false, cleanup)?;
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
        let mut native_observations = vec![native];
        for (domain, enforcement) in [
            (CapabilityDomain::Filesystem, EnforcementLevel::Hard),
            (CapabilityDomain::Process, EnforcementLevel::Supervisor),
            (CapabilityDomain::Environment, EnforcementLevel::Supervisor),
            (CapabilityDomain::Network, EnforcementLevel::Hard),
            (CapabilityDomain::Terminal, EnforcementLevel::Supervisor),
        ] {
            push_native_mapping(
                &mut native_observations,
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
            observations: vec![common],
            native_observations,
            recovery,
            cleanup,
            observation_limit,
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
        &self.native_observations
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
