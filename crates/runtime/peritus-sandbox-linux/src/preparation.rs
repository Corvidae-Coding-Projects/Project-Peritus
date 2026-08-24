//! Linux backend configuration, probe ownership, and authorized preparation adapter.

use crate::{
    HelperManifest, LinuxBackendConfig, LinuxBackendDescriptor, LinuxError, LinuxErrorKind,
    LinuxLaunchDescription, LinuxOperation, LinuxPreparedSession, LinuxProbe, LinuxRecovery,
    MountPlan, MountPolicy, NetworkIsolation, ResourcePlan,
};
use peritus_process::{
    AuthorizedPreparationContext, ErrorCode, NativeLaunchDescription, NativePlatform,
    NativeSandboxBackend, ProcessError, ProcessOperation, RecoveryClass,
};
use peritus_sandbox::BackendDescriptor;

/// Probed Linux backend. Construction performs only bounded support probes, never preparation.
#[derive(Debug)]
pub struct LinuxBackend {
    config: LinuxBackendConfig,
    probe: LinuxProbe,
    descriptor: LinuxBackendDescriptor,
}

impl LinuxBackend {
    /// Probes and freezes one Linux backend descriptor.
    ///
    /// # Errors
    /// Returns a typed probe/descriptor error when installed identities cannot be bounded.
    pub fn new(config: LinuxBackendConfig) -> Result<Self, LinuxError> {
        let probe = LinuxProbe::run(config.probe_request())?;
        let descriptor = LinuxBackendDescriptor::from_probe_with_managed_proxy(
            &probe,
            config.managed_proxy.is_some(),
        )?;
        Ok(Self { config, probe, descriptor })
    }
    /// Returns complete probe facts.
    #[must_use]
    pub const fn probe(&self) -> &LinuxProbe {
        &self.probe
    }
    /// Returns Linux-specific descriptor identity.
    #[must_use]
    pub const fn linux_descriptor(&self) -> &LinuxBackendDescriptor {
        &self.descriptor
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the authorized preparation transaction keeps all binding checks and native projections in one auditable sequence"
    )]
    fn prepare_authorized(
        mut self,
        context: &AuthorizedPreparationContext<'_>,
    ) -> Result<LinuxPreparedSession, LinuxError> {
        let execution = context.execution_plan();
        let sandbox = context.sandbox_plan();
        let admission = context.admission();
        if admission.descriptor() != self.descriptor.common()
            || admission.plan_digest() != sandbox.digest()
            || admission.descriptor_digest() != self.descriptor.common().digest()
            || admission.support_digest() != self.descriptor.common().support_digest()
            || !crate::verified::preparation_matches(
                [
                    sandbox.digest().as_bytes(),
                    self.descriptor.common().digest().as_bytes(),
                    self.descriptor.common().support_digest().as_bytes(),
                    admission.preparation_digest().as_bytes(),
                ],
                [
                    execution.sandbox_digest().as_bytes(),
                    execution.backend().descriptor_digest().as_bytes(),
                    execution.backend().support_digest().as_bytes(),
                    execution.backend().preparation_digest().as_bytes(),
                ],
            )
        {
            return Err(LinuxError::new(
                LinuxErrorKind::PreparationMismatch,
                LinuxOperation::Prepare,
                LinuxRecovery::Replan,
                "authorized Linux plan, descriptor, support, or preparation differs",
            ));
        }
        let current_probe = LinuxProbe::run(self.config.probe_request())?;
        if current_probe.digest() != self.probe.digest() {
            return Err(LinuxError::new(
                LinuxErrorKind::ProbeFailed,
                LinuxOperation::Probe,
                LinuxRecovery::Replan,
                "Linux runtime support or installed executable identity changed after selection",
            ));
        }
        if !crate::verified::support_covers(
            sandbox.required_features().bits(),
            self.descriptor.common().supported_features().bits(),
        ) {
            return Err(LinuxError::new(
                LinuxErrorKind::UnsupportedHost,
                LinuxOperation::Prepare,
                LinuxRecovery::ConfigureHost,
                "runtime probe does not cover every required sandbox feature",
            ));
        }
        let mount_policy =
            MountPolicy::new(&self.config.workspace_root, self.config.protected_roots.clone())?;
        crate::preparation_validation::validate_secret_destinations(
            sandbox,
            execution,
            &mount_policy,
        )?;
        let working_directory = std::fs::canonicalize(execution.working_directory().path())
            .map_err(|error| {
                LinuxError::io(LinuxOperation::Prepare, "revalidate working directory", &error)
            })?;
        if !working_directory.starts_with(mount_policy.workspace_root()) {
            return Err(LinuxError::new(
                LinuxErrorKind::Filesystem,
                LinuxOperation::Prepare,
                LinuxRecovery::CorrectRequest,
                "working directory is outside the resolved workspace",
            ));
        }
        let mounts = MountPlan::project(sandbox, &mount_policy)?;
        let resources = ResourcePlan::from_sandbox(sandbox);
        let pty =
            matches!(sandbox.requirements().terminal().mode(), peritus_sandbox::TerminalMode::Pty);
        let (managed_proxy, proxy_handles, mut inherited_handles) =
            crate::proxy_preparation::prepare(sandbox, &mut self.config.managed_proxy)?;
        let (exec_status, exec_status_handle, exec_status_binding) = crate::exec_status::prepare()?;
        inherited_handles.push(exec_status_binding);
        inherited_handles = crate::canonical_handles(inherited_handles)?;
        let network = if managed_proxy.is_some() {
            NetworkIsolation::ManagedProxy
        } else {
            NetworkIsolation::DenyAll
        };
        let cgroup_plan =
            crate::CgroupPlan::new(self.probe.cgroup(), admission.preparation_digest(), resources)?;
        let mut secret_session = match self.config.secrets.take() {
            Some(preparation) => Some(
                preparation
                    .prepare(
                        execution.identity().process_id(),
                        execution.identity().environment_id(),
                        sandbox.digest(),
                        execution.digest(),
                        sandbox.requirements().secrets(),
                    )
                    .map_err(|_| {
                        LinuxError::new(
                            LinuxErrorKind::PreparationMismatch,
                            LinuxOperation::Prepare,
                            LinuxRecovery::Replan,
                            "exact secret lease preparation failed after authorization",
                        )
                    })?,
            ),
            None if sandbox.requirements().secrets().is_empty() => None,
            None => {
                return Err(LinuxError::new(
                    LinuxErrorKind::UnsupportedHost,
                    LinuxOperation::Prepare,
                    LinuxRecovery::ConfigureHost,
                    "checked secret delivery requires an inert secret preparation owner",
                ));
            }
        };
        let protected_payloads = secret_session.as_ref().map_or_else(
            || Ok(Vec::new()),
            |session| {
                crate::secret::payloads_from_session(session, sandbox.requirements().secrets())
            },
        )?;
        let protected_bindings = protected_payloads
            .iter()
            .map(|payload| {
                crate::ProtectedPayloadBinding::new(
                    payload.requirement().clone(),
                    payload.manifest_handle()?,
                    payload.payload_len(),
                )
            })
            .collect::<Result<Vec<_>, LinuxError>>()?;
        let manifest = HelperManifest::new(
            sandbox.digest(),
            self.descriptor.common().digest(),
            self.descriptor.common().support_digest(),
            admission.preparation_digest(),
            crate::process::target_command(execution, sandbox)?,
            working_directory,
            cgroup_plan.leaf().to_path_buf(),
            pty,
            crate::process::environment(execution)?,
            mounts.landlock_rules().to_vec(),
            resources,
            network,
            inherited_handles,
        )?
        .with_protected_payloads(protected_bindings)?;
        let local_launch = LinuxLaunchDescription::build(
            self.config.probe_request.bubblewrap_path(),
            self.config.probe_request.helper_path(),
            self.descriptor.identity().helper_digest(),
            &mounts,
            &manifest,
        )?;
        let mut protected_handles =
            protected_payloads.iter().map(|payload| payload.handle().clone()).collect::<Vec<_>>();
        protected_handles.extend(proxy_handles);
        protected_handles.push(exec_status_handle);
        let native_launch = NativeLaunchDescription::new(
            local_launch.command().clone(),
            local_launch.helper_identity(),
            local_launch.manifest().bytes().to_vec(),
            local_launch.manifest().digest(),
            admission.preparation_digest(),
        )
        .map_err(|_| {
            LinuxError::new(
                LinuxErrorKind::Helper,
                LinuxOperation::Prepare,
                LinuxRecovery::CorrectRequest,
                "native launch description rejected the bounded helper manifest",
            )
        })?
        .with_protected_handles(protected_handles)
        .map_err(|_| {
            LinuxError::new(
                LinuxErrorKind::PreparationMismatch,
                LinuxOperation::Prepare,
                LinuxRecovery::CorrectRequest,
                "native launch rejected protected payload ownership or handle identity",
            )
        })?;
        let cgroup = cgroup_plan.install()?;
        Ok(LinuxPreparedSession::new(
            native_launch,
            local_launch,
            exec_status,
            cgroup,
            resources,
            pty,
            matches!(network, NetworkIsolation::ManagedProxy),
            managed_proxy,
            secret_session.take(),
            sandbox.digest(),
            self.descriptor.common().digest(),
            self.probe.digest(),
            admission.preparation_digest(),
        ))
    }
}

impl NativeSandboxBackend for LinuxBackend {
    type Session = LinuxPreparedSession;

    fn descriptor(&self) -> &BackendDescriptor {
        self.descriptor.common()
    }

    fn platform(&self) -> NativePlatform {
        NativePlatform::Linux
    }

    fn prepare(
        self,
        context: AuthorizedPreparationContext<'_>,
    ) -> Result<Self::Session, ProcessError> {
        self.prepare_authorized(&context).map_err(|error| process_error(&error))
    }
}

const fn process_error(error: &LinuxError) -> ProcessError {
    let (code, operation, recovery, detail) = match error.kind() {
        LinuxErrorKind::InvalidPlan | LinuxErrorKind::Filesystem => (
            ErrorCode::InvalidInput,
            ProcessOperation::Validate,
            RecoveryClass::CorrectRequest,
            "Linux preparation cannot represent the checked plan exactly",
        ),
        LinuxErrorKind::UnsupportedHost | LinuxErrorKind::ProbeFailed => (
            ErrorCode::Unsupported,
            ProcessOperation::Validate,
            RecoveryClass::SelectBackend,
            "Linux runtime probe lacks required native support",
        ),
        LinuxErrorKind::DescriptorMismatch | LinuxErrorKind::PreparationMismatch => (
            ErrorCode::PlanMismatch,
            ProcessOperation::Validate,
            RecoveryClass::SelectBackend,
            "Linux native preparation binding differs from C2 admission",
        ),
        LinuxErrorKind::Resource => (
            ErrorCode::ResourceLimit,
            ProcessOperation::Spawn,
            RecoveryClass::CancelAndReap,
            "Linux resource enforcement installation failed",
        ),
        LinuxErrorKind::RecoveryIndeterminate => (
            ErrorCode::Indeterminate,
            ProcessOperation::Reconcile,
            RecoveryClass::Quarantine,
            "Linux native ownership is indeterminate",
        ),
        LinuxErrorKind::Helper
        | LinuxErrorKind::SandboxDenied
        | LinuxErrorKind::Cgroup
        | LinuxErrorKind::Network
        | LinuxErrorKind::Observation
        | LinuxErrorKind::Io => (
            ErrorCode::Supervisor,
            ProcessOperation::Spawn,
            RecoveryClass::CancelAndReap,
            "Linux native preparation or lifecycle operation failed",
        ),
    };
    ProcessError::new(code, operation, recovery, detail)
}

pub const fn lifecycle_process_error(error: &LinuxError) -> ProcessError {
    process_error(error)
}
