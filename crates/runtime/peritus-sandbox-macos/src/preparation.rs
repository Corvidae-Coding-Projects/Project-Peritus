//! Fail-closed native preparation and C2 adapter.

use peritus_process::{
    AuthorizedPreparationContext, CommandSpec, ExecutionPlan, NativeLaunchDescription,
    NativePlatform, NativeSandboxBackend, ProcessError,
};
use peritus_sandbox::{
    AdmissionProfile, BackendAdmission, BackendDescriptor, CheckedSandboxPlan, RuleEffect,
    admit_backend,
};
use peritus_secrets::SecretDeliverySession;
use peritus_types::Sha256Digest;
use std::sync::Arc;

use crate::{
    BACKEND_NAME, BACKEND_VERSION, HelperManifest, MacosDescriptor, MacosError, MacosErrorKind,
    MacosHostProbe, MacosOperation, MacosSession, ProcessContainment, ProfileCompiler,
    ProtectedProxyRoute, ProtectedSecretHandle, RecoveryAction, ResourceControlPlan,
    TerminalMapping, error,
    session::{SessionResources, process_error},
};

mod config;
mod retained_owner;
mod resources;

pub use config::PreparationConfig;
pub use retained_owner::RetainedMacosBackendFactory;

use resources::{
    canonical_protected_roots, helper_digest, proxy_identity_digest, proxy_prepare_error,
    preflight_secret_destinations, secret_prepare_error, stage_proxy_handle, stage_secret_handles,
    validate_default_metadata_aliases, validate_secret_file_destinations,
    validate_unchanged_executable, PreparationOwners,
};

/// Fully prepared session ready for transfer to the C2 supervisor.
pub type PreparedMacosSandbox = MacosSession;

/// Monotonic, nonsensitive progress emitted during authorized native preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationProgress {
    /// All selected paths and exact plan bindings passed preflight before live owners were started.
    PreflightComplete,
    /// Installed-helper identity bytes were streamed into the integrity digest.
    HelperIntegrity {
        /// Bytes incorporated into the digest.
        bytes_hashed: u64,
        /// File length observed from the already-open helper descriptor.
        total_bytes: u64,
    },
    /// The exact managed proxy route and protected token handle were prepared.
    ProxyPrepared,
    /// Exact credential lookups and lease-bound delivery artifacts were prepared.
    SecretResolution {
        /// Checked secrets prepared so far.
        completed_secrets: u32,
        /// Total checked secret count.
        secret_count: u32,
    },
    /// One secret payload is being streamed into its protected anonymous handle.
    SecretStaging {
        /// Zero-based checked secret index.
        secret_index: u32,
        /// Total checked secret count.
        secret_count: u32,
        /// Bytes committed for this secret.
        bytes_staged: u64,
        /// Exact selected artifact length observed before streaming.
        total_bytes: u64,
    },
    /// The complete digest-bound session is ready for C2 transfer.
    Complete,
}

/// macOS backend implementation selected by a probe-derived descriptor.
pub struct MacosBackend {
    descriptor: MacosDescriptor,
    config: PreparationConfig,
    preparation_continues: Arc<dyn Fn() -> bool + Send + Sync>,
    preparation_progress: Arc<dyn Fn(PreparationProgress) + Send + Sync>,
}

impl core::fmt::Debug for MacosBackend {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("MacosBackend")
            .field("descriptor", &self.descriptor)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl MacosBackend {
    /// Creates a backend from immutable probe evidence and installation configuration.
    ///
    /// # Errors
    /// Returns a typed descriptor error only if crate-owned identity constants are invalid.
    pub fn new(probe: &MacosHostProbe, config: PreparationConfig) -> Result<Self, MacosError> {
        Self::new_cancellable(probe, config, || true)
    }

    /// Creates a backend that retains the caller's cancellation signal through preparation.
    ///
    /// # Errors
    /// Returns a typed descriptor error only if crate-owned identity constants are invalid.
    pub fn new_cancellable(
        probe: &MacosHostProbe,
        config: PreparationConfig,
        should_continue: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<Self, MacosError> {
        Self::new_with_progress(probe, config, should_continue, |_| {})
    }

    /// Creates a backend with caller-owned cancellation and explicit preparation progress.
    ///
    /// Progress contains only fixed stage identities, counts, and byte lengths. It never exposes
    /// paths, proxy credentials, secret references, or secret material.
    ///
    /// # Errors
    /// Returns a typed descriptor error only if crate-owned identity constants are invalid.
    pub fn new_with_progress(
        probe: &MacosHostProbe,
        config: PreparationConfig,
        should_continue: impl Fn() -> bool + Send + Sync + 'static,
        observe_progress: impl Fn(PreparationProgress) + Send + Sync + 'static,
    ) -> Result<Self, MacosError> {
        config.validate_managed_network_identity()?;
        let mut evidence = probe.evidence().clone();
        evidence.proxy &= config.proxy.is_some();
        evidence.credential_store &= config.secrets.is_some();
        let probe = MacosHostProbe::from_evidence(evidence)?;
        Ok(Self {
            descriptor: MacosDescriptor::from_probe(probe)?,
            config,
            preparation_continues: Arc::new(should_continue),
            preparation_progress: Arc::new(observe_progress),
        })
    }

    /// Returns the exact probe-derived C2 descriptor.
    #[must_use]
    pub const fn descriptor(&self) -> &BackendDescriptor {
        self.descriptor.descriptor()
    }

    /// Returns the exact host probe.
    #[must_use]
    pub const fn probe(&self) -> &MacosHostProbe {
        self.descriptor.probe()
    }

    /// Admits a checked plan only against probed support.
    ///
    /// # Errors
    /// Returns strict unsupported behavior for any missing feature.
    pub fn admit(&self, plan: &CheckedSandboxPlan) -> Result<BackendAdmission, MacosError> {
        admit_backend(plan, self.descriptor(), AdmissionProfile::Production).map_err(|error| {
            MacosError::new(
                MacosErrorKind::UnsupportedHost,
                MacosOperation::Probe,
                RecoveryAction::SelectSupportedBackend,
                format!("backend admission failed: {}", error.code()),
            )
        })
    }

    /// Performs authorized native preparation after C2 consumed the exact action.
    #[allow(clippy::too_many_lines, reason = "complete preparation gate remains visibly ordered")]
    fn prepare_authorized(
        self,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<PreparedMacosSandbox, MacosError> {
        self.ensure_preparation_continues()?;
        let preparation_continues = Arc::clone(&self.preparation_continues);
        let preparation_progress = Arc::clone(&self.preparation_progress);
        self.validate_bindings(execution, sandbox, admission)?;
        if sandbox.isolation() != peritus_sandbox::IsolationRequirement::Restricted {
            return Err(MacosError::new(
                MacosErrorKind::InvalidInput,
                MacosOperation::Prepare,
                RecoveryAction::CorrectRequest,
                "native macOS preparation requires restricted isolation",
            ));
        }
        if !self.descriptor.probe().core_supported() {
            return Err(MacosError::new(
                MacosErrorKind::UnsupportedHost,
                MacosOperation::Prepare,
                RecoveryAction::SelectSupportedBackend,
                "macOS 15, helper, Seatbelt, and process containment are required",
            ));
        }
        self.validate_protected_bindings(sandbox)?;
        let workspace = std::fs::canonicalize(execution.working_directory().path())
            .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?;
        if workspace != execution.working_directory().path() {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "working directory changed after authorization",
            ));
        }
        validate_default_metadata_aliases(&workspace)?;
        let additional_protected_roots =
            canonical_protected_roots(&self.config.additional_protected_roots)?;
        validate_unchanged_executable(&self.config.helper_path, "helper")?;
        validate_unchanged_executable(&self.config.seatbelt_path, "Seatbelt")?;
        preflight_secret_destinations(sandbox.requirements().secrets())?;
        preparation_progress(PreparationProgress::PreflightComplete);
        let helper_digest = helper_digest(
            &self.config.helper_path,
            &preparation_continues,
            &preparation_progress,
        )?;
        if self.descriptor.probe().evidence().helper_digest != Some(helper_digest) {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "installed helper identity changed after probe",
            ));
        }
        ensure_preparation_continues(preparation_continues.as_ref())?;
        let secret_owner = self.config.secrets.map_or_else(
            || Ok(SecretDeliverySession::new()),
            |preparation| {
                preparation
                    .prepare_cancellable(
                        execution.identity().process_id(),
                        execution.identity().environment_id(),
                        sandbox.digest(),
                        execution.digest(),
                        sandbox.requirements().secrets(),
                        || preparation_continues(),
                        |completed, total| {
                            preparation_progress(PreparationProgress::SecretResolution {
                                completed_secrets: u32::try_from(completed).unwrap_or(u32::MAX),
                                secret_count: u32::try_from(total).unwrap_or(u32::MAX),
                            });
                        },
                    )
                    .map_err(secret_prepare_error)
            },
        )?;
        let mut owners = PreparationOwners::new(secret_owner);
        ensure_preparation_continues(preparation_continues.as_ref())
            .map_err(|error| owners.cleanup(error))?;
        let protected_secrets = stage_secret_handles(
            owners.secrets(),
            sandbox.requirements().secrets(),
            &preparation_continues,
            &preparation_progress,
        )
        .map_err(|error| owners.cleanup(error))?;
        validate_secret_file_destinations(&protected_secrets)
            .map_err(|error| owners.cleanup(error))?;
        ensure_preparation_continues(preparation_continues.as_ref())
            .map_err(|error| owners.cleanup(error))?;
        if let Some(preparation) = self.config.proxy {
            let proxy = preparation
                .prepare(sandbox)
                .map_err(proxy_prepare_error)
                .map_err(|error| owners.cleanup(error))?;
            owners.set_proxy(proxy);
        }
        ensure_preparation_continues(preparation_continues.as_ref())
            .map_err(|error| owners.cleanup(error))?;
        let protected_proxy = owners
            .proxy()
            .map(stage_proxy_handle)
            .transpose()
            .map_err(|error| owners.cleanup(error))?;
        if protected_proxy.is_some() {
            preparation_progress(PreparationProgress::ProxyPrepared);
        }
        let proxy_route = protected_proxy.as_ref().map(ProtectedProxyRoute::route);
        let profile = ProfileCompiler::compile(
            sandbox,
            &workspace,
            &additional_protected_roots,
            proxy_route,
        )
        .map_err(|error| owners.cleanup(error))?;
        let requested_resources = ResourceControlPlan::from_checked_plan(
            sandbox,
            self.descriptor.probe().evidence().resources.levels(),
        );
        let resources = crate::runner::negotiate_resource_controls(&requested_resources)
            .map_err(|error| owners.cleanup(error))?;
        let containment = ProcessContainment::from_checked_plan(sandbox);
        let terminal = TerminalMapping::from_checked_plan(sandbox)
            .map_err(|error| owners.cleanup(error))?;
        let environment = crate::environment::project_environment(execution)
            .map_err(|error| owners.cleanup(error))?;
        ensure_preparation_continues(preparation_continues.as_ref())
            .map_err(|error| owners.cleanup(error))?;
        let (exec_status_owner, exec_status_handle) = crate::exec_status::prepare()
            .map_err(|error| owners.cleanup(error))?;
        let exec_status_descriptor =
            u32::try_from(exec_status_handle.raw_handle()).map_err(|_| {
                error::invalid(MacosOperation::Prepare, "helper exec status descriptor is invalid")
            })
            .map_err(|error| owners.cleanup(error))?;
        let proxy_descriptor =
            protected_proxy
                .as_ref()
                .map(ProtectedProxyRoute::descriptor)
                .transpose()
                .map_err(|error| owners.cleanup(error))?;
        let secret_descriptors = protected_secrets
            .iter()
            .map(ProtectedSecretHandle::manifest_descriptor)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| owners.cleanup(error))?;
        let manifest = HelperManifest::build(
            execution.identity().process_id(),
            sandbox,
            admission.descriptor_digest(),
            admission.support_digest(),
            admission.preparation_digest(),
            &profile,
            self.config.seatbelt_path.clone(),
            execution.command(),
            workspace,
            environment,
            exec_status_descriptor,
            proxy_descriptor,
            resources,
            containment,
            terminal,
            secret_descriptors,
        )
        .map_err(|error| owners.cleanup(error))?;
        let command = CommandSpec::new(
            self.config.helper_path.as_os_str().to_owned(),
            std::iter::empty::<std::ffi::OsString>(),
        )
            .map_err(|_| error::invalid(MacosOperation::Prepare, "helper command is invalid"))
            .map_err(|error| owners.cleanup(error))?;
        let helper_identity = helper_identity(helper_digest);
        let mut protected_handles =
            protected_secrets.iter().map(|secret| secret.handle().clone()).collect::<Vec<_>>();
        protected_handles.push(exec_status_handle);
        if let Some(proxy) = &protected_proxy {
            protected_handles.push(proxy.handle().clone());
        }
        let launch = NativeLaunchDescription::new(
            command,
            helper_identity,
            manifest.canonical_bytes().to_vec(),
            manifest.digest(),
            admission.preparation_digest(),
        )
        .and_then(|launch| launch.with_protected_handles(protected_handles))
        .map_err(|source| {
            MacosError::new(
                MacosErrorKind::PreparationMismatch,
                MacosOperation::Prepare,
                RecoveryAction::Reauthorize,
                format!(
                    "C2 rejected the native launch description ({})",
                    source.code().as_str()
                ),
            )
            .with_source(error::process_source(&source))
        })
        .map_err(|error| owners.cleanup(error))?;
        let facts = crate::verified::NativeBindingFacts {
            features_covered: sandbox
                .required_features()
                .is_subset_of(self.descriptor.descriptor().supported_features()),
            plan_exact: execution.sandbox_digest() == sandbox.digest(),
            descriptor_exact: admission.descriptor_digest()
                == self.descriptor.descriptor().digest(),
            support_exact: admission.support_digest()
                == self.descriptor.descriptor().support_digest(),
            preparation_exact: admission.preparation_digest() == manifest.preparation_digest(),
            helper_exact: self.descriptor.probe().evidence().helper_digest == Some(helper_digest),
            manifest_exact: peritus_codec::sha256(manifest.canonical_bytes()) == manifest.digest(),
            profile_exact: peritus_codec::sha256(manifest.profile().as_bytes())
                == manifest.profile_digest(),
        };
        if !crate::verified::native_binding_complete(facts) {
            let mismatch = error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "native preparation refinement binding is incomplete",
            );
            return Err(owners.cleanup(mismatch));
        }
        ensure_preparation_continues(preparation_continues.as_ref())
            .map_err(|error| owners.cleanup(error))?;
        let proxy_digest = proxy_route.map(proxy_identity_digest);
        let observation_limit =
            usize::try_from(sandbox.contract().terminal().limits().event_count().get())
                .unwrap_or(usize::MAX);
        let (proxy_owner, secret_owner) = owners.into_parts();
        let session = MacosSession::new(
            launch,
            manifest,
            helper_digest,
            proxy_digest,
            observation_limit,
            SessionResources::new_cancellable(
                exec_status_owner,
                proxy_owner,
                secret_owner,
                Arc::clone(&preparation_continues),
            ),
        )?;
        preparation_progress(PreparationProgress::Complete);
        Ok(session)
    }

    fn ensure_preparation_continues(&self) -> Result<(), MacosError> {
        ensure_preparation_continues(self.preparation_continues.as_ref())
    }

    fn validate_bindings(
        &self,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<(), MacosError> {
        let selected = execution.backend();
        let exact = execution.sandbox_digest() == sandbox.digest()
            && admission.plan_digest() == sandbox.digest()
            && admission.descriptor() == self.descriptor()
            && selected.name() == BACKEND_NAME
            && selected.version() == BACKEND_VERSION
            && selected.descriptor_digest() == self.descriptor().digest()
            && selected.support_digest() == self.descriptor().support_digest()
            && selected.preparation_digest() == admission.preparation_digest();
        if !exact {
            return Err(error::mismatch(
                MacosErrorKind::DescriptorMismatch,
                "execution, sandbox, admission, and macOS descriptor disagree",
            ));
        }
        Ok(())
    }

    fn validate_protected_bindings(&self, sandbox: &CheckedSandboxPlan) -> Result<(), MacosError> {
        let egress_allowed = sandbox
            .contract()
            .network()
            .rules()
            .iter()
            .any(|rule| rule.effect() == RuleEffect::Allow);
        if egress_allowed != self.config.proxy.is_some() {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "managed proxy payload differs from the checked network contract",
            ));
        }
        if self.config.proxy.is_some() && !self.descriptor.probe().evidence().proxy {
            return Err(MacosError::new(
                MacosErrorKind::UnsupportedHost,
                MacosOperation::Prepare,
                RecoveryAction::SelectSupportedBackend,
                "managed proxy transport was unavailable during probe",
            ));
        }
        let requirements = sandbox.requirements().secrets();
        if requirements.is_empty() != self.config.secrets.is_none() {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "secret preparation presence differs from checked requirements",
            ));
        }
        if !requirements.is_empty() && !self.descriptor.probe().evidence().credential_store {
            return Err(MacosError::new(
                MacosErrorKind::UnsupportedHost,
                MacosOperation::Prepare,
                RecoveryAction::SelectSupportedBackend,
                "macOS credential-store access was not probed",
            ));
        }
        Ok(())
    }
}

fn ensure_preparation_continues(
    should_continue: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), MacosError> {
    if should_continue() {
        Ok(())
    } else {
        Err(MacosError::new(
            MacosErrorKind::SupervisorFailure,
            MacosOperation::Prepare,
            RecoveryAction::CancelAndReap,
            "native preparation was cancelled by its caller",
        ))
    }
}

impl NativeSandboxBackend for MacosBackend {
    type Session = MacosSession;

    fn descriptor(&self) -> &BackendDescriptor {
        self.descriptor()
    }

    fn platform(&self) -> NativePlatform {
        NativePlatform::Macos
    }

    fn retained_factory_request(
        &self,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<peritus_process::RetainedBackendFactoryRequest, ProcessError> {
        retained_owner::request(self, sandbox, admission)
    }

    fn prepare(
        self,
        context: AuthorizedPreparationContext<'_>,
    ) -> Result<Self::Session, ProcessError> {
        self.prepare_authorized(
            context.execution_plan(),
            context.sandbox_plan(),
            context.admission(),
        )
        .map_err(|error| {
            let cleanup_complete = error.preparation_cleanup().is_complete();
            process_error(&error).with_preparation_cleanup(cleanup_complete)
        })
    }
}

fn helper_identity(digest: Sha256Digest) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut identity = format!("{BACKEND_NAME}:{BACKEND_VERSION}:");
    for byte in digest.as_bytes() {
        identity.push(char::from(HEX[usize::from(byte >> 4)]));
        identity.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    identity
}
