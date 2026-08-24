//! Linux-specific A2 subject backed by real projections and inert session transitions.

use crate::{
    CgroupHandle, EnvironmentEntry, HelperManifest, InheritedHandle, LinuxLaunchDescription,
    LinuxPreparedSession, MountPlan, MountPolicy, NetworkIsolation, ProtectedPayloadBinding,
    ResourcePlan, TargetCommand,
};
use peritus_conformance::{
    SandboxConformanceError, SandboxConformanceFixture, SandboxConformanceObservation,
    SandboxConformanceSubject, SandboxDecision, SandboxLifecyclePhase, SandboxPreparationFixture,
    SandboxPreparationObservation,
};
use peritus_process::{
    CancellationReason, NativeLaunchDescription, NativeSandboxSession, OsExitObservation,
    ProcessTreeIdentity,
};
use peritus_sandbox::{
    AdmissionProfile, BackendAdmission, BackendDescriptor, BackendKind, BackendName,
    BackendVersion, CheckedSandboxPlan, PathSemantics, ResourceFidelity, TerminalMode,
    admit_backend,
};
use std::path::{Path, PathBuf};

pub(super) struct LinuxConformanceSubject {
    _root: tempfile::TempDir,
    workspace: PathBuf,
}

#[derive(Clone, Copy)]
pub(super) struct SessionOutcome {
    decision: SandboxDecision,
    cancellation: bool,
    process_tree_contained: bool,
    terminal_controlled: bool,
    resource_observed: u64,
}

impl SessionOutcome {
    pub(super) const fn new(
        decision: SandboxDecision,
        cancellation: bool,
        process_tree_contained: bool,
        terminal_controlled: bool,
        resource_observed: u64,
    ) -> Self {
        Self {
            decision,
            cancellation,
            process_tree_contained,
            terminal_controlled,
            resource_observed,
        }
    }
}

impl LinuxConformanceSubject {
    pub(super) fn new() -> Result<Self, std::io::Error> {
        let root = tempfile::tempdir()?;
        let workspace = root.path().join("workspace");
        std::fs::create_dir_all(workspace.join("src"))?;
        std::fs::write(workspace.join("src/lib.rs"), b"conformance\n")?;
        std::fs::write(workspace.join("conformance"), b"canonical\n")?;
        std::fs::create_dir_all(workspace.join(".cgroup"))?;
        Ok(Self { _root: root, workspace })
    }

    pub(super) fn workspace(&self) -> &Path {
        &self.workspace
    }

    pub(super) fn descriptor(plan: &CheckedSandboxPlan) -> BackendDescriptor {
        BackendDescriptor::new(
            BackendName::new(crate::BACKEND_NAME).expect("fixed backend name"),
            BackendVersion::new(crate::BACKEND_VERSION).expect("fixed backend version"),
            BackendKind::Native,
            PathSemantics::UnixNative,
            ResourceFidelity::Hard,
            plan.required_features(),
        )
    }

    pub(super) fn project_manifest(
        &self,
        plan: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<(MountPlan, HelperManifest), ()> {
        let policy = MountPolicy::new(&self.workspace, Vec::new()).map_err(|_| ())?;
        let mounts = MountPlan::project(plan, &policy).map_err(|_| ())?;
        let resources = ResourcePlan::from_sandbox(plan);
        let managed_network = !plan.requirements().network().is_empty();
        let inherited_handles = if managed_network {
            vec![
                InheritedHandle::new(201, crate::PROXY_LISTENER_LABEL.to_owned())
                    .map_err(|_| ())?,
                InheritedHandle::new(202, crate::PROXY_TOKEN_LABEL.to_owned()).map_err(|_| ())?,
            ]
        } else {
            Vec::new()
        };
        let cgroup_leaf = self.workspace.join(".cgroup").join(format!(
            "peritus-{}",
            &crate::canonical::digest_hex(admission.preparation_digest())[..24]
        ));
        let pty = plan.requirements().terminal().mode() == TerminalMode::Pty;
        let mut manifest = HelperManifest::new(
            plan.digest(),
            admission.descriptor_digest(),
            admission.support_digest(),
            admission.preparation_digest(),
            TargetCommand::new("/bin/true".to_owned(), Vec::new()).map_err(|_| ())?,
            self.workspace.clone(),
            cgroup_leaf,
            pty,
            Vec::<EnvironmentEntry>::new(),
            mounts.landlock_rules().to_vec(),
            resources,
            if managed_network {
                NetworkIsolation::ManagedProxy
            } else {
                NetworkIsolation::DenyAll
            },
            inherited_handles,
        )
        .map_err(|_| ())?;
        if let Some(requirement) = plan.requirements().secrets().first() {
            manifest = manifest
                .with_protected_payloads(vec![
                    ProtectedPayloadBinding::new(
                        requirement.clone(),
                        InheritedHandle::new(203, "peritus-secret-environment-v1-0".to_owned())
                            .map_err(|_| ())?,
                        37,
                    )
                    .map_err(|_| ())?,
                ])
                .map_err(|_| ())?;
        }
        let encoded = manifest.encode().map_err(|_| ())?;
        let decoded = HelperManifest::decode(&encoded).map_err(|_| ())?;
        if decoded != manifest {
            return Err(());
        }
        Ok((mounts, decoded))
    }

    fn session(&self, plan: &CheckedSandboxPlan) -> Result<LinuxPreparedSession, ()> {
        let descriptor = Self::descriptor(plan);
        let admission =
            admit_backend(plan, &descriptor, AdmissionProfile::Conformance).map_err(|_| ())?;
        let (mounts, manifest) = self.project_manifest(plan, &admission)?;
        let local = LinuxLaunchDescription::build(
            Path::new("/usr/bin/bwrap"),
            Path::new("/usr/libexec/peritus-linux-sandbox-helper"),
            peritus_codec::sha256(b"linux-conformance-helper"),
            &mounts,
            &manifest,
        )
        .map_err(|_| ())?;
        let (exec_status, exec_status_handle, _) = crate::exec_status::prepare().map_err(|_| ())?;
        let launch = NativeLaunchDescription::new(
            local.command().clone(),
            local.helper_identity(),
            local.manifest().bytes().to_vec(),
            local.manifest().digest(),
            admission.preparation_digest(),
        )
        .map_err(|_| ())?
        .with_protected_handles(vec![exec_status_handle])
        .map_err(|_| ())?;
        let cgroup_root = self.workspace.join(".cgroup");
        let cgroup = CgroupHandle::reopen_exact(cgroup_root, manifest.cgroup_leaf().to_path_buf());
        Ok(LinuxPreparedSession::new(
            launch,
            local,
            exec_status,
            cgroup,
            ResourcePlan::from_sandbox(plan),
            manifest.expects_pty(),
            manifest.network() == NetworkIsolation::ManagedProxy,
            None,
            None,
            plan.digest(),
            descriptor.digest(),
            peritus_codec::sha256(b"linux-conformance-probe"),
            admission.preparation_digest(),
        ))
    }

    pub(super) fn run_session(
        &self,
        plan: &CheckedSandboxPlan,
        fixture: &SandboxConformanceFixture,
        outcome: SessionOutcome,
    ) -> Result<SandboxConformanceObservation, ()> {
        let mut session = self.session(plan)?;
        session
            .activated(ProcessTreeIdentity::new(17, Some(19), Some(17), true))
            .map_err(|_| ())?;
        if outcome.cancellation {
            session.cancellation_requested(CancellationReason::User).map_err(|_| ())?;
        }
        session.terminated(&OsExitObservation::Code(0)).map_err(|_| ())?;
        session.release().map_err(|_| ())?;
        let observations = session.observations();
        let observed_plan = observations
            .first()
            .map_or([0; 32], |observation| *observation.plan_digest().as_bytes());
        let activation_count = observations
            .iter()
            .filter(|observation| observation.kind() == peritus_sandbox::ObservationKind::Activated)
            .count() as u64;
        let mut ordinary = session.launch_description().manifest().to_vec();
        ordinary.extend_from_slice(format!("{:?}", session.linux_observations()).as_bytes());
        Ok(SandboxConformanceObservation::new(
            outcome.decision,
            SandboxLifecyclePhase::Released,
            Vec::new(),
            outcome.resource_observed,
            fixture.resource_limit(),
            activation_count,
            0,
            outcome.cancellation,
            true,
            *plan.digest().as_bytes(),
            observed_plan,
            observations.iter().map(|observation| observation.sequence()).collect(),
            ordinary,
            outcome.process_tree_contained,
            outcome.terminal_controlled,
        ))
    }
}

impl SandboxConformanceSubject for LinuxConformanceSubject {
    fn exercise(
        &mut self,
        fixture: &SandboxConformanceFixture,
    ) -> Result<SandboxConformanceObservation, SandboxConformanceError> {
        super::exercise::exercise(self, fixture)
            .map_err(|()| SandboxConformanceError::Infrastructure)
    }

    fn prepare(
        &mut self,
        fixture: &SandboxPreparationFixture,
    ) -> Result<SandboxPreparationObservation, SandboxConformanceError> {
        super::preparation::prepare(self, fixture)
            .map_err(|()| SandboxConformanceError::Infrastructure)
    }
}
