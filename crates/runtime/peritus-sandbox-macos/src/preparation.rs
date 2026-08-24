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

use crate::{
    BACKEND_NAME, BACKEND_VERSION, HelperManifest, MacosDescriptor, MacosError, MacosErrorKind,
    MacosHostProbe, MacosOperation, MacosSession, ProcessContainment, ProfileCompiler,
    ProtectedProxyRoute, ProtectedSecretHandle, RecoveryAction, ResourceControlPlan,
    TerminalMapping, error,
    session::{SessionResources, process_error},
};

mod config;
mod resources;

pub use config::PreparationConfig;

use resources::{
    canonical_protected_roots, proxy_identity_digest, proxy_prepare_error, read_bounded_helper,
    secret_prepare_error, stage_proxy_handle, stage_secret_handles,
    validate_default_metadata_aliases, validate_secret_file_destinations,
    validate_unchanged_executable,
};

/// Fully prepared session ready for transfer to the C2 supervisor.
pub type PreparedMacosSandbox = MacosSession;

/// macOS backend implementation selected by a probe-derived descriptor.
#[derive(Debug)]
pub struct MacosBackend {
    descriptor: MacosDescriptor,
    config: PreparationConfig,
}

impl MacosBackend {
    /// Creates a backend from immutable probe evidence and installation configuration.
    ///
    /// # Errors
    /// Returns a typed descriptor error only if crate-owned identity constants are invalid.
    pub fn new(probe: &MacosHostProbe, config: PreparationConfig) -> Result<Self, MacosError> {
        let mut evidence = probe.evidence().clone();
        evidence.proxy &= config.proxy.is_some();
        evidence.credential_store &= config.secrets.is_some();
        let probe = MacosHostProbe::from_evidence(evidence)?;
        Ok(Self { descriptor: MacosDescriptor::from_probe(probe)?, config })
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
        self.validate_bindings(execution, sandbox, admission)?;
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
        let helper_bytes = read_bounded_helper(&self.config.helper_path)?;
        let helper_digest = peritus_codec::sha256(&helper_bytes);
        if self.descriptor.probe().evidence().helper_digest != Some(helper_digest) {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "installed helper identity changed after probe",
            ));
        }
        let proxy_owner = self
            .config
            .proxy
            .map(|preparation| preparation.prepare(sandbox).map_err(|_| proxy_prepare_error()))
            .transpose()?;
        let protected_proxy = proxy_owner.as_ref().map(stage_proxy_handle).transpose()?;
        let proxy_route = protected_proxy.as_ref().map(ProtectedProxyRoute::route);
        let secret_owner = self.config.secrets.map_or_else(
            || Ok(SecretDeliverySession::new()),
            |preparation| {
                preparation
                    .prepare(
                        execution.identity().process_id(),
                        execution.identity().environment_id(),
                        sandbox.digest(),
                        execution.digest(),
                        sandbox.requirements().secrets(),
                    )
                    .map_err(|_| secret_prepare_error())
            },
        )?;
        let protected_secrets = stage_secret_handles(&secret_owner)?;
        validate_secret_file_destinations(&protected_secrets)?;
        let profile = ProfileCompiler::compile(
            sandbox,
            &workspace,
            &additional_protected_roots,
            proxy_route,
        )?;
        let resources = ResourceControlPlan::from_checked_plan(
            sandbox,
            self.descriptor.probe().evidence().resources.levels(),
        );
        let containment = ProcessContainment::from_checked_plan(sandbox);
        let terminal = TerminalMapping::from_checked_plan(sandbox)?;
        let environment = crate::environment::project_environment(execution)?;
        let (exec_status_owner, exec_status_handle) = crate::exec_status::prepare()?;
        let exec_status_descriptor =
            u32::try_from(exec_status_handle.raw_handle()).map_err(|_| {
                error::invalid(MacosOperation::Prepare, "helper exec status descriptor is invalid")
            })?;
        let proxy_descriptor =
            protected_proxy.as_ref().map(ProtectedProxyRoute::descriptor).transpose()?;
        let secret_descriptors = protected_secrets
            .iter()
            .map(ProtectedSecretHandle::manifest_descriptor)
            .collect::<Result<Vec<_>, _>>()?;
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
        )?;
        let helper_path = self.config.helper_path.to_str().ok_or_else(|| {
            error::invalid(MacosOperation::Prepare, "helper path is not valid UTF-8")
        })?;
        let command = CommandSpec::new(helper_path, std::iter::empty::<String>())
            .map_err(|_| error::invalid(MacosOperation::Prepare, "helper command is invalid"))?;
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
        .map_err(|_| {
            MacosError::new(
                MacosErrorKind::PreparationMismatch,
                MacosOperation::Prepare,
                RecoveryAction::Reauthorize,
                "C2 rejected the native launch description",
            )
        })?;
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
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "native preparation refinement binding is incomplete",
            ));
        }
        let proxy_digest = proxy_route.map(proxy_identity_digest);
        let observation_limit =
            usize::try_from(sandbox.contract().terminal().limits().event_count().get())
                .unwrap_or(usize::MAX);
        MacosSession::new(
            launch,
            manifest,
            helper_digest,
            proxy_digest,
            observation_limit,
            SessionResources::new(exec_status_owner, proxy_owner, secret_owner),
        )
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

impl NativeSandboxBackend for MacosBackend {
    type Session = MacosSession;

    fn descriptor(&self) -> &BackendDescriptor {
        self.descriptor()
    }

    fn platform(&self) -> NativePlatform {
        NativePlatform::Macos
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
        .map_err(|error| process_error(&error))
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
