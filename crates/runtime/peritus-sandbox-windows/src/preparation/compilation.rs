//! Effect-free binding, capacity, path, and manifest validation for Windows preparation.
use super::WindowsBackend;
use crate::{
    EnvironmentEntry, HelperManifest, InheritedHandlePolicy, JobPlan, PathPolicy, ProcessPolicy,
    TerminalMapping, WindowsError, WindowsErrorKind, WindowsLaunchDescription, WindowsOperation,
    compile_acl_plan,
};
use peritus_process::ExecutionPlan;
use peritus_sandbox::{BackendAdmission, CheckedSandboxPlan};
use peritus_types::Sha256Digest;
use std::sync::Arc;

impl WindowsBackend {
    #[allow(
        clippy::too_many_lines,
        reason = "single inert projection shared by pre-consumption and preparation"
    )]
    pub(super) fn preflight_compilation(
        &self,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
        install_native: bool,
    ) -> Result<(crate::AclPlan, crate::channels::PreparedChannelPlan, HelperManifest), WindowsError>
    {
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
        #[cfg(target_os = "windows")]
        for entry in acl.entries() {
            let authority = crate::ResolvedWindowsPath::resolve(entry.authority_root().clone())?;
            let (anchor, exists) =
                crate::ResolvedWindowsPath::resolve_existing_or_parent(entry.path().clone())?;
            if authority.evidence().volume_serial() != anchor.evidence().volume_serial()
                || (!exists && !entry.creates_deny_directory())
            {
                return Err(crate::error::invalid(
                    WindowsOperation::ResolvePath,
                    "ACL target cannot be represented within its existing native authority",
                ));
            }
        }

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
            environment,
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
            helper_identity,
            projected_manifest.clone(),
            Vec::new(),
        )?;
        Ok((acl, channel_plan, projected_manifest))
    }

    pub(super) fn validate_bindings(
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

    pub(super) fn ensure_preparation_continues(&self) -> Result<(), WindowsError> {
        if (self.preparation_continues)() { Ok(()) } else { Err(crate::probe::probe_cancelled()) }
    }

    pub(super) fn validate_selected_capacity(
        &self,
        sandbox: &CheckedSandboxPlan,
    ) -> Result<(), WindowsError> {
        self.ensure_preparation_continues()?;
        let selected = self.descriptor.probe().selected_controls(sandbox, &self.config.token)?;
        let helper_digest = helper_digest(&self.config.helper_path, &self.preparation_continues)?;
        if self.descriptor.probe().evidence().helper_digest != Some(helper_digest) {
            return Err(crate::error::mismatch(
                WindowsErrorKind::PreparationMismatch,
                "installed helper identity changed before authority consumption",
            ));
        }
        let network_selected = !sandbox.requirements().network().is_empty();
        if network_selected
            && (self.config.proxy.is_none() || !self.descriptor.probe().evidence().managed_network)
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
            None | Some(_) if requirements.is_empty() => {}
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
                matches!(requirement.delivery(), peritus_sandbox::SecretDelivery::BrokeredHandle(_))
            })
            .count();
        InheritedHandlePolicy::new(
            (1..=inherited_secret_count).map(|value| value as u64).collect(),
        )?;
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

    pub(super) fn validate_compilation(
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
        crate::ResolvedWindowsPath::resolve(self.config.workspace.clone())?;
        for input in &self.config.read_only_inputs {
            crate::ResolvedWindowsPath::resolve(input.clone())?;
        }
        for input in &self.config.writable_inputs {
            crate::ResolvedWindowsPath::resolve(input.clone())?;
        }
        Ok(())
    }
}

fn helper_digest(
    path: &std::path::Path,
    should_continue: &Arc<dyn Fn() -> bool + Send + Sync>,
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
            sandbox.requirements().network().is_empty() && profile.is_app_container()
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

pub(super) fn target_handles(
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
