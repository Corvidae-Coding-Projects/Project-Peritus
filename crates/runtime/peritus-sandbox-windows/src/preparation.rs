//! Probe ownership, C2 binding validation, and authorized Windows preparation.

use peritus_process::{
    AuthorizedPreparationContext, ExecutionPlan, NativePlatform, NativeSandboxBackend, ProcessError,
};
use peritus_sandbox::{
    AdmissionProfile, BackendAdmission, BackendDescriptor, CheckedSandboxPlan, admit_backend,
};
use peritus_types::Sha256Digest;

use crate::{
    EnvironmentEntry, HelperManifest, InheritedHandlePolicy, JobPlan, ObservationBinding,
    PathPolicy, ProcessPolicy, ResourceControlPlan, RuntimeIdentity, TerminalMapping,
    WindowsBackendConfig, WindowsBackendDescriptor, WindowsError, WindowsErrorKind,
    WindowsLaunchDescription, WindowsOperation, WindowsProbe, WindowsSession, compile_acl_plan,
};

/// Probed Windows backend selected by C2 admission.
#[derive(Debug)]
pub struct WindowsBackend {
    config: WindowsBackendConfig,
    descriptor: WindowsBackendDescriptor,
}

impl WindowsBackend {
    /// Probes the current host and freezes its exact descriptor.
    ///
    /// # Errors
    /// Returns typed probe/descriptor failure.
    pub fn new(config: WindowsBackendConfig) -> Result<Self, WindowsError> {
        let request = crate::ProbeRequest::new(
            config.helper_path.clone(),
            config.token.clone(),
            config.managed_filter_digest(),
        )?;
        let probe = WindowsProbe::run(&request)?;
        Self::from_probe(config, probe)
    }

    /// Builds from already validated probe evidence for deterministic conformance tests.
    ///
    /// # Errors
    /// Rejects descriptor/filter inconsistency.
    pub fn from_probe(
        config: WindowsBackendConfig,
        probe: WindowsProbe,
    ) -> Result<Self, WindowsError> {
        let descriptor =
            WindowsBackendDescriptor::from_probe(probe, config.managed_filter_digest())?;
        Ok(Self { config, descriptor })
    }

    /// Returns common C2 descriptor.
    #[must_use]
    pub const fn descriptor(&self) -> &BackendDescriptor {
        self.descriptor.common()
    }
    /// Returns exact Windows descriptor/probe identity.
    #[must_use]
    pub const fn windows_descriptor(&self) -> &WindowsBackendDescriptor {
        &self.descriptor
    }

    /// Admits a checked plan strictly against probe-derived support.
    ///
    /// # Errors
    /// Returns unsupported for any missing feature.
    pub fn admit(&self, plan: &CheckedSandboxPlan) -> Result<BackendAdmission, WindowsError> {
        admit_backend(plan, self.descriptor(), AdmissionProfile::Production).map_err(|_| {
            let _no_effect = crate::verified::unsupported_has_no_effect(false, false, false);
            crate::error::unsupported(
                WindowsOperation::Probe,
                "Windows backend does not cover every checked sandbox feature",
            )
        })
    }

    /// Builds an inert prepared session for platform-neutral integration tests.
    ///
    /// Production callers use the opaque [`NativeSandboxBackend`] context, which additionally
    /// installs native temporary ACLs after durable C2 consumption.
    ///
    /// # Errors
    /// Rejects any identity drift, unsupported control, path/ACL mismatch, or protected-channel
    /// mismatch before returning a launch description.
    pub fn prepare_checked(
        mut self,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<WindowsSession, WindowsError> {
        self.prepare_internal(execution, sandbox, admission, false)
    }

    #[allow(clippy::too_many_lines, reason = "complete preparation transaction remains auditable")]
    fn prepare_internal(
        &mut self,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
        install_native: bool,
    ) -> Result<WindowsSession, WindowsError> {
        self.validate_bindings(execution, sandbox, admission)?;
        let probe = self.descriptor.probe();
        if !probe.core_supported() {
            return Err(crate::error::unsupported(
                WindowsOperation::Prepare,
                "Windows 11 24H2/Server 2025 native controls are incomplete",
            ));
        }
        let helper_bytes = std::fs::read(&self.config.helper_path)
            .map_err(|_| crate::error::io(WindowsOperation::Prepare, "helper cannot be read"))?;
        let helper_digest = peritus_codec::sha256(&helper_bytes);
        if probe.evidence().helper_digest != Some(helper_digest) {
            return Err(crate::error::mismatch(
                WindowsErrorKind::PreparationMismatch,
                "installed helper identity changed after probe",
            ));
        }
        #[cfg(target_os = "windows")]
        self.validate_native_paths(execution)?;
        let path_policy =
            PathPolicy::new(self.config.workspace.clone(), self.config.protected_roots.clone())?;
        let acl = compile_acl_plan(sandbox, &path_policy, self.config.token.principal_sid())?;
        let environment = execution
            .environment()
            .variables()
            .iter()
            .map(|value| EnvironmentEntry::new(value.name(), value.value()))
            .collect::<Result<Vec<_>, _>>()?;
        let channels = crate::channels::PreparedChannels::prepare(
            &mut self.config,
            execution,
            sandbox,
            install_native,
            probe.evidence().managed_network,
        )?;
        let terminal = TerminalMapping::from_checked_plan(sandbox)?;
        if matches!(terminal, TerminalMapping::ConPty { .. }) && !probe.evidence().conpty {
            return Err(crate::error::unsupported(
                WindowsOperation::Prepare,
                "checked terminal requires unavailable ConPTY support",
            ));
        }
        let resources = ResourceControlPlan::from_checked_plan(sandbox, probe.evidence().resources);
        let job = JobPlan::from_checked_plan(sandbox);
        let process = ProcessPolicy::from_checked_plan(sandbox);
        let inherited_handles = target_handles(&channels.secrets)?;
        let manifest = HelperManifest::build(
            execution.identity().process_id(),
            sandbox,
            admission,
            helper_digest,
            &acl,
            self.config.token.clone(),
            execution.command(),
            self.config.workspace.clone(),
            environment,
            job,
            process,
            terminal,
            resources,
            channels.network,
            channels.secrets.clone(),
            inherited_handles,
        )?;
        self.validate_compilation(execution, sandbox, admission, helper_digest, &acl, &manifest)?;
        let helper_identity = crate::identity::helper(helper_digest);
        let (windows_launch, native_launch) = WindowsLaunchDescription::new(
            &self.config.helper_path,
            helper_identity,
            manifest,
            channels.handles,
        )?;
        #[cfg(target_os = "windows")]
        let native_launch = if install_native {
            WindowsLaunchDescription::attach_helper_channels(
                native_launch,
                peritus_process::NativeWindowsHelperChannels::new().map_err(|_| {
                    crate::error::io(
                        WindowsOperation::Prepare,
                        "Windows helper status/control channels cannot be created",
                    )
                })?,
            )?
        } else {
            native_launch
        };
        let acl_transaction = if install_native {
            #[cfg(target_os = "windows")]
            {
                acl.install(&self.config.acl_backup_root)?
            }
            #[cfg(not(target_os = "windows"))]
            {
                return Err(crate::error::unsupported(
                    WindowsOperation::Prepare,
                    "Windows native ACL installation is unavailable on this host",
                ));
            }
        } else {
            acl.planned()
        };
        let binding = ObservationBinding::new(
            sandbox.digest(),
            self.descriptor().digest(),
            probe.digest(),
            admission.preparation_digest(),
        );
        let runtime_identity = RuntimeIdentity::new(
            execution.identity().process_id(),
            admission.preparation_digest(),
            helper_digest,
            crate::identity::job(admission.preparation_digest(), job),
            crate::identity::profile(&self.config.token),
            acl.digest(),
        );
        Ok(WindowsSession::new(
            native_launch,
            windows_launch,
            acl_transaction,
            resources,
            binding,
            runtime_identity,
            channels.proxy_owner,
            channels.filter_owner,
            channels.secret_owner,
        ))
    }

    fn validate_bindings(
        &self,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<(), WindowsError> {
        let feature_match =
            sandbox.required_features().is_subset_of(self.descriptor().supported_features());
        let plan_match = execution.sandbox_digest() == sandbox.digest()
            && admission.plan_digest() == sandbox.digest();
        let descriptor_match = admission.descriptor() == self.descriptor();
        let support_match = admission.support_digest() == self.descriptor().support_digest();
        let preparation_match = crate::manifest::expected_preparation(
            sandbox.digest(),
            self.descriptor().digest(),
            self.descriptor().support_digest(),
        ) == admission.preparation_digest();
        if feature_match && plan_match && descriptor_match && support_match && preparation_match {
            Ok(())
        } else {
            Err(crate::error::mismatch(
                WindowsErrorKind::PreparationMismatch,
                "authorized Windows plan, descriptor, support, or installation differs",
            ))
        }
    }

    fn validate_compilation(
        &self,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
        helper_digest: Sha256Digest,
        acl: &crate::AclPlan,
        manifest: &HelperManifest,
    ) -> Result<(), WindowsError> {
        let execution_plan_exact = execution.sandbox_digest() == manifest.plan_digest();
        let admission_plan_exact = admission.plan_digest() == manifest.plan_digest();
        let plan_exact = execution_plan_exact && admission_plan_exact;
        let helper_exact = helper_digest == manifest.helper_digest()
            && self.descriptor.identity().helper_digest() == helper_digest;
        let facts = crate::verified::NativeBindingFacts {
            features_covered: sandbox
                .required_features()
                .is_subset_of(self.descriptor().supported_features()),
            plan_exact,
            descriptor_exact: admission.descriptor_digest() == manifest.descriptor_digest(),
            support_exact: admission.support_digest() == manifest.support_digest(),
            preparation_exact: admission.preparation_digest() == manifest.preparation_digest(),
            helper_exact,
            workspace_exact: manifest.working_directory() == &self.config.workspace,
            token_exact: manifest.token() == &self.config.token,
            acl_exact: acl.digest() == manifest.acl_digest(),
            network_exact: network_exact(
                manifest.network(),
                sandbox,
                &self.config.token,
                self.config.managed_filter_digest,
            ),
            handles_exact: manifest.inherited_handles().digest()
                == target_handles(manifest.secret_handles())?.digest(),
        };
        if crate::verified::native_binding_complete(facts) {
            Ok(())
        } else {
            Err(crate::error::mismatch(
                WindowsErrorKind::PreparationMismatch,
                "compiled helper controls differ from checked and admitted native facts",
            ))
        }
    }

    #[cfg(target_os = "windows")]
    fn validate_native_paths(&self, execution: &ExecutionPlan) -> Result<(), WindowsError> {
        let working =
            crate::WindowsPath::new(execution.working_directory().path().to_string_lossy())?;
        if working != self.config.workspace {
            return Err(crate::error::mismatch(
                WindowsErrorKind::PreparationMismatch,
                "working directory changed after authorization",
            ));
        }
        let workspace = crate::ResolvedWindowsPath::resolve(self.config.workspace.clone())?;
        for protected in &self.config.protected_roots {
            let resolved = crate::ResolvedWindowsPath::resolve(protected.clone())?;
            if workspace.evidence().volume_serial() != resolved.evidence().volume_serial() {
                return Err(crate::error::invalid(
                    WindowsOperation::ResolvePath,
                    "protected root is on another volume",
                ));
            }
        }
        Ok(())
    }
}

fn network_exact(
    isolation: crate::NetworkIsolation,
    sandbox: &CheckedSandboxPlan,
    profile: &crate::TokenProfile,
    controller: Option<Sha256Digest>,
) -> bool {
    match isolation {
        crate::NetworkIsolation::DenyAll => {
            sandbox.requirements().network().is_empty()
                && controller.is_none()
                && profile.is_app_container()
        }
        crate::NetworkIsolation::ManagedProxy(route) => {
            !sandbox.requirements().network().is_empty()
                && route.network_plan_digest() == sandbox.digest()
                && controller.is_some_and(|identity| {
                    crate::network::managed_wfp_policy_digest(
                        identity,
                        profile.principal_sid(),
                        route.endpoint(),
                        sandbox.digest(),
                    ) == route.filter_digest()
                })
        }
    }
}

impl NativeSandboxBackend for WindowsBackend {
    type Session = WindowsSession;

    fn descriptor(&self) -> &BackendDescriptor {
        self.descriptor()
    }

    fn platform(&self) -> NativePlatform {
        NativePlatform::Windows
    }

    fn prepare(
        mut self,
        context: AuthorizedPreparationContext<'_>,
    ) -> Result<Self::Session, ProcessError> {
        self.prepare_internal(
            context.execution_plan(),
            context.sandbox_plan(),
            context.admission(),
            true,
        )
        .map_err(|error| crate::session::process_error(&error))
    }
}

fn target_handles(
    secrets: &[crate::ProtectedSecretHandle],
) -> Result<InheritedHandlePolicy, WindowsError> {
    let handles = secrets
        .iter()
        .filter(|handle| {
            matches!(handle.destination(), crate::SecretHandleDestination::Brokered(_))
        })
        .map(crate::ProtectedSecretHandle::handle)
        .collect();
    InheritedHandlePolicy::new(handles)
}
