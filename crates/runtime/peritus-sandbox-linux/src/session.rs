//! C2-owned Linux native session lifecycle.

use crate::{
    CgroupHandle, EnforcementLevel, LinuxError, LinuxErrorKind, LinuxLaunchDescription,
    LinuxObservation, LinuxOperation, LinuxRecovery, NativeCapability, NativePhase,
    ObservationOutcome, ResourceEnforcement, ResourcePlan, observation::ObservationBinding,
};
use peritus_process::{
    CancellationReason, NativeLaunchDescription, NativeSandboxSession, OsExitObservation,
    ProcessError, ProcessTreeIdentity,
};
use peritus_sandbox::{EnforcementObservation, SandboxResourceKind};
use peritus_types::Sha256Digest;

const RICH_OBSERVATION_BOUND: usize = 64;

fn release_emits_observation(phase: NativePhase) -> Result<bool, LinuxError> {
    match phase {
        NativePhase::Prepared | NativePhase::Terminated => Ok(true),
        NativePhase::Released => Ok(false),
        NativePhase::Activated | NativePhase::CancelRequested => Err(LinuxError::new(
            LinuxErrorKind::Observation,
            LinuxOperation::Release,
            LinuxRecovery::CancelAndReap,
            "native resources cannot be released before target termination",
        )),
    }
}

/// Prepared native resources retained by the C2 supervisor through release.
#[derive(Debug)]
pub struct LinuxPreparedSession {
    launch: NativeLaunchDescription,
    linux_launch: LinuxLaunchDescription,
    exec_status: Option<crate::exec_status::ExecStatusOwner>,
    cgroup: Option<CgroupHandle>,
    managed_proxy: Option<crate::network::ManagedProxyOwner>,
    secrets: Option<peritus_secrets::SecretDeliverySession>,
    phase: NativePhase,
    binding: ObservationBinding,
    observations: Vec<EnforcementObservation>,
    linux_observations: Vec<LinuxObservation>,
    resource_enforcement: Vec<ResourceEnforcement>,
}

impl LinuxPreparedSession {
    #[allow(clippy::too_many_arguments, reason = "complete prepared-session identity")]
    pub(crate) fn new(
        launch: NativeLaunchDescription,
        linux_launch: LinuxLaunchDescription,
        exec_status: crate::exec_status::ExecStatusOwner,
        cgroup: CgroupHandle,
        resources: ResourcePlan,
        pty: bool,
        proxy_route: bool,
        managed_proxy: Option<crate::network::ManagedProxyOwner>,
        secrets: Option<peritus_secrets::SecretDeliverySession>,
        plan_digest: Sha256Digest,
        backend_digest: Sha256Digest,
        probe_digest: Sha256Digest,
        preparation_digest: Sha256Digest,
    ) -> Self {
        let binding = ObservationBinding {
            plan: plan_digest,
            backend: backend_digest,
            probe: probe_digest,
            preparation: preparation_digest,
        };
        let observations =
            vec![binding.common(1, NativePhase::Prepared, ObservationOutcome::Observed)];
        let mut linux_observations = vec![LinuxObservation::new(
            1,
            binding,
            NativePhase::Prepared,
            None,
            None,
            ObservationOutcome::Observed,
        )];
        for capability in [
            NativeCapability::Namespaces,
            NativeCapability::Landlock,
            NativeCapability::Seccomp,
            NativeCapability::PrivilegeDrop,
            NativeCapability::Cgroup,
        ] {
            let sequence = u64::try_from(linux_observations.len() + 1).unwrap_or(u64::MAX);
            linux_observations.push(LinuxObservation::new(
                sequence,
                binding,
                NativePhase::Prepared,
                Some(capability),
                Some(EnforcementLevel::Hard),
                ObservationOutcome::Observed,
            ));
        }
        for (present, capability, level) in [
            (pty, NativeCapability::Pty, EnforcementLevel::Supervisor),
            (proxy_route, NativeCapability::ProxyRoute, EnforcementLevel::Hard),
        ] {
            if present {
                let sequence = u64::try_from(linux_observations.len() + 1).unwrap_or(u64::MAX);
                linux_observations.push(LinuxObservation::new(
                    sequence,
                    binding,
                    NativePhase::Prepared,
                    Some(capability),
                    Some(level),
                    ObservationOutcome::Observed,
                ));
            }
        }
        let resource_enforcement: Vec<_> = [
            SandboxResourceKind::WallTime,
            SandboxResourceKind::CpuTime,
            SandboxResourceKind::Memory,
            SandboxResourceKind::Disk,
            SandboxResourceKind::Output,
            SandboxResourceKind::OpenHandles,
            SandboxResourceKind::Processes,
            SandboxResourceKind::Concurrency,
        ]
        .into_iter()
        .map(|kind| ResourceEnforcement::new(kind, resources.enforcement(kind)))
        .collect();
        for enforcement in &resource_enforcement {
            let sequence = u64::try_from(linux_observations.len() + 1).unwrap_or(u64::MAX);
            linux_observations.push(LinuxObservation::new(
                sequence,
                binding,
                NativePhase::Prepared,
                Some(NativeCapability::Resource(enforcement.kind())),
                Some(enforcement.level()),
                ObservationOutcome::Observed,
            ));
        }
        Self {
            launch,
            linux_launch,
            exec_status: Some(exec_status),
            cgroup: Some(cgroup),
            managed_proxy,
            secrets,
            phase: NativePhase::Prepared,
            binding,
            observations,
            linux_observations,
            resource_enforcement,
        }
    }
    /// Returns backend-local structured launch details.
    #[must_use]
    pub const fn linux_launch_description(&self) -> &LinuxLaunchDescription {
        &self.linux_launch
    }
    /// Returns rich preparation-bound Linux observations.
    #[must_use]
    pub fn linux_observations(&self) -> &[LinuxObservation] {
        &self.linux_observations
    }
    /// Returns dimension-specific truthful resource enforcement.
    #[must_use]
    pub fn resource_enforcement(&self) -> &[ResourceEnforcement] {
        &self.resource_enforcement
    }

    fn transition(
        &mut self,
        next: NativePhase,
        outcome: ObservationOutcome,
    ) -> Result<(), LinuxError> {
        if !crate::verified::lifecycle_transition_allowed(self.phase, next) {
            return Err(LinuxError::new(
                LinuxErrorKind::Observation,
                LinuxOperation::Observe,
                LinuxRecovery::Reconcile,
                "native lifecycle transition is out of order",
            ));
        }
        let common_sequence = u64::try_from(self.observations.len() + 1).map_err(|_| {
            LinuxError::new(
                LinuxErrorKind::Observation,
                LinuxOperation::Observe,
                LinuxRecovery::Reconcile,
                "native observation sequence overflowed",
            )
        })?;
        self.observations.push(self.binding.common(common_sequence, next, outcome));
        if self.linux_observations.len() >= RICH_OBSERVATION_BOUND {
            return Err(LinuxError::new(
                LinuxErrorKind::Observation,
                LinuxOperation::Observe,
                LinuxRecovery::Reconcile,
                "Linux observation bound was exhausted",
            ));
        }
        let rich_sequence = u64::try_from(self.linux_observations.len() + 1).unwrap_or(u64::MAX);
        self.linux_observations.push(LinuxObservation::new(
            rich_sequence,
            self.binding,
            next,
            None,
            None,
            outcome,
        ));
        self.phase = next;
        Ok(())
    }
}

impl NativeSandboxSession for LinuxPreparedSession {
    fn launch_description(&self) -> &NativeLaunchDescription {
        &self.launch
    }

    fn observations(&self) -> &[EnforcementObservation] {
        &self.observations
    }

    fn activated(&mut self, tree: ProcessTreeIdentity) -> Result<(), ProcessError> {
        if !tree.complete_containment() {
            return Err(crate::preparation::lifecycle_process_error(&LinuxError::new(
                LinuxErrorKind::SandboxDenied,
                LinuxOperation::Activate,
                LinuxRecovery::CancelAndReap,
                "C2 did not observe complete process-tree containment",
            )));
        }
        if self.cgroup.is_none() {
            return Err(crate::preparation::lifecycle_process_error(&LinuxError::new(
                LinuxErrorKind::Cgroup,
                LinuxOperation::Attach,
                LinuxRecovery::Reconcile,
                "cgroup handle is absent before activation",
            )));
        }
        if !self.launch.release_protected_handle(crate::EXEC_STATUS_LABEL) {
            return Err(crate::preparation::lifecycle_process_error(&LinuxError::new(
                LinuxErrorKind::Helper,
                LinuxOperation::Activate,
                LinuxRecovery::CancelAndReap,
                "helper execution-status handle is absent after launch",
            )));
        }
        let mut exec_status = self.exec_status.take().ok_or_else(|| {
            crate::preparation::lifecycle_process_error(&LinuxError::new(
                LinuxErrorKind::Helper,
                LinuxOperation::Activate,
                LinuxRecovery::CancelAndReap,
                "helper execution-status owner is absent after launch",
            ))
        })?;
        exec_status
            .observe(self.launch.manifest_digest(), self.launch.preparation_digest())
            .map_err(|error| crate::preparation::lifecycle_process_error(&error))?;
        self.transition(NativePhase::Activated, ObservationOutcome::Observed)
            .map_err(|error| crate::preparation::lifecycle_process_error(&error))
    }

    fn cancellation_requested(&mut self, _reason: CancellationReason) -> Result<(), ProcessError> {
        if self.phase == NativePhase::CancelRequested {
            return Ok(());
        }
        self.transition(NativePhase::CancelRequested, ObservationOutcome::Observed)
            .map_err(|error| crate::preparation::lifecycle_process_error(&error))
    }

    fn terminated(&mut self, _exit: &OsExitObservation) -> Result<(), ProcessError> {
        self.transition(NativePhase::Terminated, ObservationOutcome::Observed)
            .map_err(|error| crate::preparation::lifecycle_process_error(&error))
    }

    fn release(&mut self) -> Result<(), ProcessError> {
        if self.phase == NativePhase::Released {
            return Ok(());
        }
        let emit_observation = release_emits_observation(self.phase)
            .map_err(|error| crate::preparation::lifecycle_process_error(&error))?;
        if let Some(mut cgroup) = self.cgroup.take()
            && let Err(error) = cgroup.cleanup()
        {
            self.cgroup = Some(cgroup);
            return Err(crate::preparation::lifecycle_process_error(&error));
        }
        crate::network::shutdown_managed_proxy(&mut self.managed_proxy)
            .map_err(|error| crate::preparation::lifecycle_process_error(&error))?;
        if let Some(secrets) = self.secrets.as_mut()
            && secrets.release().is_err()
        {
            return Err(crate::preparation::lifecycle_process_error(&LinuxError::new(
                LinuxErrorKind::SandboxDenied,
                LinuxOperation::Release,
                LinuxRecovery::RetryCleanup,
                "secret delivery cleanup remained incomplete",
            )));
        }
        self.secrets = None;
        self.exec_status = None;
        let launch_without_handles = NativeLaunchDescription::new(
            self.launch.command().clone(),
            self.launch.helper_identity(),
            self.launch.manifest().to_vec(),
            self.launch.manifest_digest(),
            self.launch.preparation_digest(),
        )?;
        self.launch = launch_without_handles;
        if !emit_observation {
            return Ok(());
        }
        self.transition(NativePhase::Released, ObservationOutcome::Observed)
            .map_err(|error| crate::preparation::lifecycle_process_error(&error))
    }
}

#[cfg(test)]
mod tests {
    use super::release_emits_observation;
    use crate::NativePhase;

    #[test]
    fn prepared_validation_failure_cleans_up_without_normal_release_observation() {
        assert!(release_emits_observation(NativePhase::Prepared).expect("prepared cleanup"));
        assert!(release_emits_observation(NativePhase::Terminated).expect("normal cleanup"));
        assert!(release_emits_observation(NativePhase::Activated).is_err());
        assert!(release_emits_observation(NativePhase::CancelRequested).is_err());
    }
}
