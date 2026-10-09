//! Probe ownership, C2 binding validation, and authorized Windows preparation.

use peritus_process::{
    AuthorizedPreparationContext, ExecutionPlan, NativePlatform, NativeSandboxBackend, ProcessError,
};
use peritus_sandbox::{
    AdmissionProfile, BackendAdmission, BackendDescriptor, CheckedSandboxPlan, admit_backend,
};
use std::sync::Arc;

use crate::{
    AclTransaction, HelperManifest, ObservationBinding, PreparationCleanup, RuntimeIdentity,
    WindowsBackendConfig, WindowsBackendDescriptor, WindowsError, WindowsLaunchDescription,
    WindowsOperation, WindowsProbe, WindowsSession,
};

#[cfg(target_os = "windows")]
use crate::WindowsErrorKind;

mod compilation;
use compilation::target_handles;

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

    fn take_handles(
        &mut self,
    ) -> Result<Vec<peritus_process::NativeProtectedHandle>, WindowsError> {
        self.channels.as_mut().map(|channels| core::mem::take(&mut channels.handles)).ok_or_else(
            || {
                crate::error::invalid(
                    WindowsOperation::Prepare,
                    "staged protected-handle ownership is absent",
                )
            },
        )
    }

    fn cleanup(&mut self, mut original: WindowsError) -> WindowsError {
        if let Some(channels) = self.channels.as_mut() {
            original = channels.cleanup(original);
        }
        let acl_restore = self.acl.restore().is_err() || !self.acl.restored();
        let original =
            original.with_cleanup(PreparationCleanup::new([acl_restore, false, false, false]));
        if acl_restore {
            let digest = self.acl.digest();
            let acl = core::mem::replace(&mut self.acl, AclTransaction::planned(digest));
            original.retain_cleanup(crate::error::CleanupOwner::Acl(Box::new(acl)))
        } else {
            original
        }
    }

    fn finish(
        mut self,
    ) -> Result<(AclTransaction, crate::channels::PreparedChannels), WindowsError> {
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
        self.descriptor.probe().selected_controls(plan, &self.config.token)?;
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
        let (acl, channel_plan, projected) =
            self.preflight_compilation(execution, sandbox, admission, install_native)?;
        let helper_digest = projected.helper_digest();
        let helper_identity = crate::identity::helper(helper_digest);
        let environment = projected.environment().to_vec();
        let job = projected.job();
        let process = projected.process();
        let terminal = projected.terminal();
        let resources = projected.resources();
        let probe = self.descriptor.probe();
        #[cfg(target_os = "windows")]
        let native_helper_channels = if install_native {
            Some(peritus_process::NativeWindowsHelperChannels::new().map_err(|source| {
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
        self.ensure_preparation_continues().map_err(|error| staged.cleanup(error))?;
        let (network, secrets) = match staged.channels() {
            Ok(channels) => (channels.network, channels.secrets.clone()),
            Err(error) => return Err(staged.cleanup(error)),
        };
        let inherited_handles = target_handles(&secrets).map_err(|error| staged.cleanup(error))?;
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
            crate::identity::job(admission.preparation_digest(), job),
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
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<(), ProcessError> {
        self.validate_bindings(execution, sandbox, admission)
            .and_then(|()| self.validate_selected_capacity(sandbox))
            .and_then(|()| {
                self.preflight_compilation(execution, sandbox, admission, true).map(|_| ())
            })
            .map_err(|error| crate::session::process_error(&error).with_source(error))
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
        .map_err(|error| {
            let cleanup_complete = error.preparation_cleanup().is_complete();
            crate::session::process_error(&error)
                .with_preparation_cleanup(cleanup_complete)
                .with_source(error)
        })
    }
}
