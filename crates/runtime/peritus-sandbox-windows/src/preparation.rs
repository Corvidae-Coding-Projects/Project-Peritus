//! Probe ownership, C2 binding validation, and authorized Windows preparation.

use peritus_process::{
    AuthorizedPreparationContext, ExecutionPlan, NativePlatform, NativeSandboxBackend, ProcessError,
    RetainedOwnerBinding,
};
use peritus_sandbox::{
    AdmissionProfile, BackendAdmission, BackendDescriptor, CheckedSandboxPlan, admit_backend,
};
use peritus_types::Sha256Digest;
use std::sync::Arc;

use crate::{
    AclTransaction, EnvironmentEntry, HelperManifest, InheritedHandlePolicy, JobPlan,
    ObservationBinding, PathPolicy, PreparationCleanup, ProcessPolicy, RuntimeIdentity,
    TerminalMapping,
    WindowsBackendConfig, WindowsBackendDescriptor, WindowsError, WindowsErrorKind,
    WindowsLaunchDescription, WindowsOperation, WindowsProbe, WindowsSession, compile_acl_plan,
};

mod retained_owner;

pub use retained_owner::RetainedWindowsBackendFactory;

struct StagedPreparation {
    acl: AclTransaction,
    channels: Option<crate::channels::PreparedChannels>,
}

impl StagedPreparation {
    const fn new(acl: AclTransaction) -> Self {
        Self { acl, channels: None }
    }

    fn set_channels(&mut self, channels: crate::channels::PreparedChannels) {
        self.channels = Some(channels);
    }

    fn channels(&self) -> Result<&crate::channels::PreparedChannels, WindowsError> {
        self.channels.as_ref().ok_or_else(|| {
            crate::error::invalid(
                WindowsOperation::Prepare,
                "staged protected-channel ownership is absent",
            )
        })
    }

    fn take_handles(&mut self) -> Result<Vec<peritus_process::NativeProtectedHandle>, WindowsError> {
        self.channels.as_mut().map(|channels| {
            core::mem::take(&mut channels.handles)
        }).ok_or_else(|| {
            crate::error::invalid(
                WindowsOperation::Prepare,
                "staged protected-handle ownership is absent",
            )
        })
    }

    fn cleanup(&mut self, mut original: WindowsError) -> WindowsError {
        if let Some(channels) = self.channels.as_mut() {
            original = channels.cleanup(original);
        }
        let acl_restore = self.acl.restore().is_err() || !self.acl.restored();
        original.with_cleanup(PreparationCleanup::new(
            acl_restore,
            false,
            false,
            false,
        ))
    }

    fn finish(mut self) -> Result<(AclTransaction, crate::channels::PreparedChannels), WindowsError> {
        let channels = self.channels.take().ok_or_else(|| {
            crate::error::invalid(
                WindowsOperation::Prepare,
                "completed preparation lacks protected-channel ownership",
            )
        })?;
        Ok((self.acl, channels))
    }
}

/// Probed Windows backend selected by C2 admission.
pub struct WindowsBackend {
    config: WindowsBackendConfig,
    descriptor: WindowsBackendDescriptor,
    preparation_continues: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl core::fmt::Debug for WindowsBackend {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("WindowsBackend")
            .field("config", &self.config)
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

impl WindowsBackend {
    /// Probes the current host and freezes its exact descriptor.
    ///
    /// # Errors
    /// Returns typed probe/descriptor failure.
    pub fn new(config: WindowsBackendConfig) -> Result<Self, WindowsError> {
        Self::new_cancellable(config, || true)
    }

    /// Probes the current host while the caller retains cancellation ownership.
    ///
    /// # Errors
    /// Returns typed cancellation, probe, or descriptor failure.
    pub fn new_cancellable(
        config: WindowsBackendConfig,
        should_continue: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<Self, WindowsError> {
        config.validate_managed_network_identity()?;
        let request = crate::ProbeRequest::new(
            config.helper_path.clone(),
            config.token.clone(),
            config.managed_filter_digest(),
        )?
        .with_acl_probe_root(config.acl_backup_root.clone())?;
        let preparation_continues = Arc::new(should_continue);
        let probe = WindowsProbe::run_cancellable(&request, || preparation_continues())?;
        let descriptor =
            WindowsBackendDescriptor::from_probe(probe, config.managed_filter_digest())?;
        Ok(Self { config, descriptor, preparation_continues })
    }

    /// Builds from already validated probe evidence for deterministic conformance tests.
    ///
    /// # Errors
    /// Rejects descriptor/filter inconsistency.
    pub fn from_probe(
        config: WindowsBackendConfig,
        probe: WindowsProbe,
    ) -> Result<Self, WindowsError> {
        config.validate_managed_network_identity()?;
        let descriptor =
            WindowsBackendDescriptor::from_probe(probe, config.managed_filter_digest())?;
        Ok(Self { config, descriptor, preparation_continues: Arc::new(|| true) })
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
        self.descriptor
            .probe()
            .selected_controls(plan, &self.config.token)?;
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
        self.prepare_internal(execution, sandbox, admission, false, None)
    }

    #[allow(clippy::too_many_lines, reason = "complete preparation transaction remains auditable")]
    fn prepare_internal(
        &mut self,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
        install_native: bool,
        retained_owner: Option<RetainedOwnerBinding>,
    ) -> Result<WindowsSession, WindowsError> {
        self.ensure_preparation_continues()?;
        self.validate_bindings(execution, sandbox, admission)?;
        if sandbox.isolation() != peritus_sandbox::IsolationRequirement::Restricted {
            return Err(crate::error::invalid(
                WindowsOperation::Prepare,
                "native Windows preparation requires restricted isolation",
            ));
        }
        let probe = self.descriptor.probe();
        let selected = probe.selected_controls(sandbox, &self.config.token)?;
        let helper_digest = helper_digest(&self.config.helper_path, &self.preparation_continues)?;
        if probe.evidence().helper_digest != Some(helper_digest) {
            return Err(crate::error::mismatch(
                WindowsErrorKind::PreparationMismatch,
                "installed helper identity changed after probe",
            ));
        }
        #[cfg(target_os = "windows")]
        self.validate_native_paths(execution)?;
        let path_policy =
            PathPolicy::new(self.config.workspace.clone(), self.config.protected_roots.clone())?
                .with_read_only_inputs(self.config.read_only_inputs.clone())?
                .with_writable_inputs(self.config.writable_inputs.clone())?;
        let acl = compile_acl_plan(sandbox, &path_policy, self.config.token.principal_sid())?;
        let environment = execution
            .environment()
            .variables()
            .iter()
            .map(|value| EnvironmentEntry::new(value.name(), value.value()))
            .collect::<Result<Vec<_>, _>>()?;
        let terminal = TerminalMapping::from_checked_plan(sandbox)?;
        if matches!(terminal, TerminalMapping::ConPty { .. }) && !selected.conpty() {
            return Err(crate::error::unsupported(
                WindowsOperation::Prepare,
                "checked terminal requires unavailable ConPTY support",
            ));
        }
        let resources = selected.resources();
        let job = JobPlan::from_checked_plan(sandbox);
        let job_identity = crate::identity::job(
            execution.identity().process_id(),
            admission.preparation_digest(),
            job,
        );
        let process = ProcessPolicy::from_checked_plan(sandbox);
        let channel_plan = crate::channels::PreparedChannelPlan::preflight(
            &self.config,
            execution,
            sandbox,
            install_native,
            selected.managed_network(),
            selected.credential_delivery(),
        )?;
        let projected_inherited_handles = target_handles(channel_plan.secrets())?;
        let projected_manifest = HelperManifest::build(
            execution.identity().process_id(),
            sandbox,
            admission,
            helper_digest,
            &acl,
            self.config.token.clone(),
            execution.command(),
            self.config.workspace.clone(),
            environment.clone(),
            job,
            process,
            terminal,
            resources,
            channel_plan.network(),
            channel_plan.secrets().to_vec(),
            projected_inherited_handles,
        )?;
        self.validate_compilation(
            execution,
            sandbox,
            admission,
            helper_digest,
            &acl,
            &projected_manifest,
        )?;
        let helper_identity = crate::identity::helper(helper_digest);
        let _preflight_launch = WindowsLaunchDescription::new(
            &self.config.helper_path,
            helper_identity.clone(),
            projected_manifest,
            Vec::new(),
        )?;
        #[cfg(target_os = "windows")]
        let native_helper_channels = if install_native {
            let object_name =
                crate::identity::job_name(execution.identity().process_id(), job_identity);
            let containment_job = crate::native::prepare_containment_job(job, &object_name)?;
            Some(peritus_process::NativeWindowsHelperChannels::new_with_containment(
                containment_job,
                job_identity,
                object_name,
            ).map_err(|source| {
                WindowsError::new(
                    WindowsErrorKind::Handle,
                    WindowsOperation::Prepare,
                    crate::WindowsRecovery::CancelAndReap,
                    "Windows helper status/control channels cannot be created",
                )
                .with_source(crate::error::process_source(&source))
            })?)
        } else {
            None
        };
        self.ensure_preparation_continues()?;
        let acl_transaction = if install_native {
            #[cfg(target_os = "windows")]
            {
                acl.install(
                    &self.config.acl_backup_root,
                    execution.identity().process_id(),
                    admission.preparation_digest(),
                    retained_owner,
                    self.preparation_continues.as_ref(),
                )?
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
        let mut staged = StagedPreparation::new(acl_transaction);
        let channels = crate::channels::PreparedChannels::prepare(
            &mut self.config,
            execution,
            sandbox,
            &channel_plan,
            self.preparation_continues.as_ref(),
        )
        .map_err(|error| staged.cleanup(error))?;
        staged.set_channels(channels);
        self.ensure_preparation_continues()
            .map_err(|error| staged.cleanup(error))?;
        let (network, secrets) = match staged.channels() {
            Ok(channels) => (channels.network, channels.secrets.clone()),
            Err(error) => return Err(staged.cleanup(error)),
        };
        let inherited_handles =
            target_handles(&secrets).map_err(|error| staged.cleanup(error))?;
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
            network,
            secrets,
            inherited_handles,
        )
        .map_err(|error| staged.cleanup(error))?;
        self.validate_compilation(execution, sandbox, admission, helper_digest, &acl, &manifest)
            .map_err(|error| staged.cleanup(error))?;
        let protected_handles = match staged.take_handles() {
            Ok(handles) => handles,
            Err(error) => return Err(staged.cleanup(error)),
        };
        let (windows_launch, native_launch) = WindowsLaunchDescription::new(
            &self.config.helper_path,
            helper_identity,
            manifest,
            protected_handles,
        )
        .map_err(|error| staged.cleanup(error))?;
        #[cfg(target_os = "windows")]
        let native_launch = if let Some(helper_channels) = native_helper_channels {
            WindowsLaunchDescription::attach_helper_channels(native_launch, helper_channels)
                .map_err(|error| staged.cleanup(error))?
        } else {
            native_launch
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
            job_identity,
            crate::identity::profile(&self.config.token),
            acl.digest(),
        );
        let (acl_transaction, channels) = staged.finish()?;
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

    fn ensure_preparation_continues(&self) -> Result<(), WindowsError> {
        if (self.preparation_continues)() {
            Ok(())
        } else {
            Err(crate::probe::probe_cancelled())
        }
    }

    fn validate_selected_capacity(
        &self,
        sandbox: &CheckedSandboxPlan,
    ) -> Result<(), WindowsError> {
        self.ensure_preparation_continues()?;
        let selected = self
            .descriptor
            .probe()
            .selected_controls(sandbox, &self.config.token)?;
        let helper_digest = helper_digest(&self.config.helper_path, &self.preparation_continues)?;
        if self.descriptor.probe().evidence().helper_digest != Some(helper_digest) {
            return Err(crate::error::mismatch(
                WindowsErrorKind::PreparationMismatch,
                "installed helper identity changed before authority consumption",
            ));
        }
        let network_selected = !sandbox.requirements().network().is_empty();
        if network_selected
            && (self.config.proxy.is_none()
                || !self.descriptor.probe().evidence().managed_network)
        {
            return Err(crate::error::unsupported(
                WindowsOperation::Prepare,
                "selected network control lacks exact inert preparation or native capacity",
            ));
        }
        let requirements = sandbox.requirements().secrets();
        if !requirements.is_empty() && !selected.credential_delivery() {
            return Err(crate::error::unsupported(
                WindowsOperation::Prepare,
                "selected secret delivery lacks qualified credential-store support",
            ));
        }
        match &self.config.secrets {
            Some(preparation) if preparation.lease_count() == requirements.len() => {}
            None if requirements.is_empty() => {}
            Some(_) if requirements.is_empty() => {}
            Some(_) | None => {
                return Err(crate::error::mismatch(
                    WindowsErrorKind::PreparationMismatch,
                    "selected secret deliveries differ from supplied exact leases",
                ));
            }
        }
        let inherited_secret_count = requirements
            .iter()
            .filter(|requirement| {
                matches!(
                    requirement.delivery(),
                    peritus_sandbox::SecretDelivery::BrokeredHandle(_)
                )
            })
            .count();
        InheritedHandlePolicy::validate_count(inherited_secret_count)?;
        if let TerminalMapping::ConPty { columns, rows, .. } =
            TerminalMapping::from_checked_plan(sandbox)?
            && (i16::try_from(columns).is_err() || i16::try_from(rows).is_err())
        {
            return Err(crate::error::unsupported(
                WindowsOperation::Prepare,
                "selected ConPTY dimensions exceed the native coordinate representation",
            ));
        }
        let _resources = selected.resources();
        JobPlan::from_checked_plan(sandbox)
            .validate_native_capacity(WindowsOperation::Prepare)?;
        #[cfg(target_os = "windows")]
        {
            self.validate_configured_native_paths()?;
            let mut should_continue = || (self.preparation_continues)();
            crate::native::probe::validate_selected_capacity(
                sandbox,
                &self.config.token,
                &mut should_continue,
            )?;
        }
        Ok(())
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
        let working = crate::WindowsPath::from_canonicalized(execution.working_directory().path())?;
        if !working.same_native_path(&self.config.workspace) {
            return Err(crate::error::mismatch(
                WindowsErrorKind::PreparationMismatch,
                "working directory changed after authorization",
            ));
        }
        self.validate_configured_native_paths()
    }

    #[cfg(target_os = "windows")]
    fn validate_configured_native_paths(&self) -> Result<(), WindowsError> {
        let workspace = crate::ResolvedWindowsPath::resolve(self.config.workspace.clone())?;
        for input in &self.config.read_only_inputs {
            crate::ResolvedWindowsPath::resolve(input.clone())?;
        }
        for input in &self.config.writable_inputs {
            crate::ResolvedWindowsPath::resolve(input.clone())?;
        }
        for protected in &self.config.protected_roots {
            let (anchor, _) =
                crate::ResolvedWindowsPath::resolve_existing_or_parent(protected.clone())?;
            if workspace.evidence().volume_serial() != anchor.evidence().volume_serial() {
                return Err(crate::error::invalid(
                    WindowsOperation::ResolvePath,
                    "protected root is on another volume",
                ));
            }
        }
        Ok(())
    }
}

fn helper_digest(
    path: &std::path::Path,
    should_continue: &impl Fn() -> bool,
) -> Result<Sha256Digest, WindowsError> {
    let mut continuation = || should_continue();
    match crate::probe::inspect_helper_image(path, &mut continuation) {
        Ok(image) if image.bytes() != 0 => Ok(image.digest()),
        Ok(_) | Err(crate::probe::HelperImageFailure::Unavailable) => Err(crate::error::io(
            WindowsOperation::Prepare,
            "helper image cannot be read as one finite regular file",
        )),
        Err(crate::probe::HelperImageFailure::Changed) => Err(crate::error::mismatch(
            WindowsErrorKind::PreparationMismatch,
            "helper image length changed during streamed verification",
        )),
        Err(crate::probe::HelperImageFailure::Cancelled) => Err(crate::probe::probe_cancelled()),
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

    fn validate_preparation_capacity(
        &self,
        sandbox: &CheckedSandboxPlan,
    ) -> Result<(), ProcessError> {
        self.validate_selected_capacity(sandbox)
            .map_err(|error| crate::session::process_error(&error))
    }

    fn retained_factory_request(
        &self,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<peritus_process::RetainedBackendFactoryRequest, ProcessError> {
        retained_owner::request(self, sandbox, admission)
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
            context.retained_owner(),
        )
        .map_err(|error| {
            let cleanup_complete = error.preparation_cleanup().is_complete();
            crate::session::process_error(&error)
                .with_preparation_cleanup(cleanup_complete)
        })
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
