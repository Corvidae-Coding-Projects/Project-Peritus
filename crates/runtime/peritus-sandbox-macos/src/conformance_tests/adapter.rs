//! macOS-specific A2 subject backed by real projections and inert session transitions.

use std::path::{Path, PathBuf};

use peritus_conformance::{
    SandboxConformanceError, SandboxConformanceFixture, SandboxConformanceObservation,
    SandboxConformanceSubject, SandboxDecision, SandboxLifecyclePhase, SandboxPreparationFixture,
    SandboxPreparationObservation,
};
use peritus_network::{
    DnsMode, ManagedProxy, NetworkBounds, NetworkPlan, ProxyMode, RedirectMode, RoutingToken,
    RuntimeNetworkOptions,
};
use peritus_process::{
    CancellationReason, CommandSpec, NativeLaunchDescription, NativeProtectedHandle,
    NativeSandboxSession, OsExitObservation, ProcessTreeIdentity,
};
use peritus_sandbox::{
    AdmissionProfile, BackendAdmission, CheckedSandboxPlan, ObservationKind, admit_backend,
};
use peritus_secrets::{
    SecretDeliveryContext, SecretDeliverySession, SecretLease, SecretLeaseId, SecretMaterial,
};
use peritus_types::Sha256Digest;

use crate::{
    HelperManifest, MacosDescriptor, MacosHostProbe, MacosSession, ProcessContainment,
    ProfileCompiler, ProtectedProxyRoute, ProtectedSecretHandle, ResourceControlPlan,
    SecretHandleDestination, TerminalMapping,
};

pub(super) struct MacosConformanceSubject {
    _root: tempfile::TempDir,
    workspace: PathBuf,
    descriptor: MacosDescriptor,
}

#[derive(Clone, Copy)]
pub(super) struct SessionOutcome {
    pub decision: SandboxDecision,
    pub cancellation: bool,
    pub process_tree_contained: bool,
    pub terminal_controlled: bool,
    pub resource_observed: u64,
}

impl MacosConformanceSubject {
    pub(super) fn new() -> Result<Self, ()> {
        let root = tempfile::tempdir().map_err(|_| ())?;
        let workspace = root.path().join("workspace");
        std::fs::create_dir_all(workspace.join("src")).map_err(|_| ())?;
        std::fs::write(workspace.join("src/lib.rs"), b"conformance\n").map_err(|_| ())?;
        std::fs::write(workspace.join("conformance"), b"canonical\n").map_err(|_| ())?;
        let probe = MacosHostProbe::from_evidence(crate::ProbeEvidence::supported_fixture())
            .map_err(|_| ())?;
        let descriptor = MacosDescriptor::from_probe(probe).map_err(|_| ())?;
        Ok(Self { _root: root, workspace, descriptor })
    }

    pub(super) fn workspace(&self) -> &Path {
        &self.workspace
    }

    pub(super) fn descriptor(&self) -> &MacosDescriptor {
        &self.descriptor
    }

    pub(super) fn project(
        &self,
        plan: &CheckedSandboxPlan,
        admission: &BackendAdmission,
        secret_canary: &[u8],
    ) -> Result<PreparedProjection, ()> {
        let (proxy, protected_proxy) = prepare_proxy(plan)?;
        let proxy_route = protected_proxy.as_ref().map(ProtectedProxyRoute::route);
        let (secrets, protected_secrets) =
            self.prepare_secrets(plan, admission.preparation_digest(), secret_canary)?;
        let profile =
            ProfileCompiler::compile(plan, &self.workspace, &[], proxy_route).map_err(|_| ())?;
        let proxy_descriptor = protected_proxy
            .as_ref()
            .map(ProtectedProxyRoute::descriptor)
            .transpose()
            .map_err(|_| ())?;
        let secret_descriptors = protected_secrets
            .iter()
            .map(ProtectedSecretHandle::manifest_descriptor)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ())?;
        let (exec_status, exec_status_handle) = crate::exec_status::prepare().map_err(|_| ())?;
        let exec_status_descriptor =
            u32::try_from(exec_status_handle.raw_handle()).map_err(|_| ())?;
        let command =
            CommandSpec::new("/bin/true", std::iter::empty::<String>()).map_err(|_| ())?;
        let manifest = HelperManifest::build(
            plan.binding().process_id(),
            plan,
            admission.descriptor_digest(),
            admission.support_digest(),
            admission.preparation_digest(),
            &profile,
            PathBuf::from("/usr/bin/sandbox-exec"),
            &command,
            self.workspace.clone(),
            Vec::new(),
            exec_status_descriptor,
            proxy_descriptor,
            ResourceControlPlan::from_checked_plan(
                plan,
                self.descriptor.probe().evidence().resources.levels(),
            ),
            ProcessContainment::from_checked_plan(plan),
            TerminalMapping::from_checked_plan(plan).map_err(|_| ())?,
            secret_descriptors,
        )
        .map_err(|_| ())?;
        let decoded = HelperManifest::decode(manifest.canonical_bytes()).map_err(|_| ())?;
        if decoded != manifest {
            return Err(());
        }
        let helper_digest = peritus_codec::sha256(b"macos-conformance-helper");
        let launch = NativeLaunchDescription::new(
            CommandSpec::new(
                "/usr/libexec/peritus-macos-sandbox-helper",
                std::iter::empty::<String>(),
            )
            .map_err(|_| ())?,
            "peritus-macos-conformance-helper-v1",
            manifest.canonical_bytes().to_vec(),
            manifest.digest(),
            admission.preparation_digest(),
        )
        .and_then(|launch| {
            let mut handles =
                protected_secrets.iter().map(|secret| secret.handle().clone()).collect::<Vec<_>>();
            if let Some(proxy) = &protected_proxy {
                handles.push(proxy.handle().clone());
            }
            handles.push(exec_status_handle);
            launch.with_protected_handles(handles)
        })
        .map_err(|_| ())?;
        Ok(PreparedProjection {
            launch,
            manifest,
            helper_digest,
            proxy_digest: proxy_route
                .map(|route| peritus_codec::sha256(route.endpoint().to_string().as_bytes())),
            exec_status,
            proxy,
            secrets,
        })
    }

    fn prepare_secrets(
        &self,
        plan: &CheckedSandboxPlan,
        execution_digest: Sha256Digest,
        secret_canary: &[u8],
    ) -> Result<(SecretDeliverySession, Vec<ProtectedSecretHandle>), ()> {
        let mut owner = SecretDeliverySession::new();
        let context = SecretDeliveryContext::new(
            plan.binding().process_id(),
            plan.binding().environment_id(),
            plan.digest(),
            execution_digest,
            1,
        );
        let mut handles = Vec::new();
        for (index, requirement) in plan.requirements().secrets().iter().enumerate() {
            let marker = u8::try_from(index).map_err(|_| ())?.saturating_add(1);
            let lease = SecretLease::new(
                SecretLeaseId::new([marker; 16]),
                plan.binding().process_id(),
                plan.binding().environment_id(),
                plan.digest(),
                execution_digest,
                requirement.reference(),
                requirement.delivery().clone(),
                1,
                2,
            )
            .map_err(|_| ())?;
            owner
                .deliver(
                    lease,
                    SecretMaterial::new(secret_canary.to_vec()).map_err(|_| ())?,
                    context,
                    &self.workspace.join(".secrets"),
                )
                .map_err(|_| ())?;
            let native = NativeProtectedHandle::from_bytes(
                format!("peritus-macos-conformance-secret-{index}"),
                secret_canary.to_vec(),
            )
            .map_err(|_| ())?;
            handles.push(
                ProtectedSecretHandle::new(
                    native,
                    requirement.reference(),
                    SecretHandleDestination::from(requirement.delivery()),
                )
                .map_err(|_| ())?,
            );
        }
        Ok((owner, crate::canonical_secret_handles(handles).map_err(|_| ())?))
    }

    pub(super) fn run_session(
        &self,
        plan: &CheckedSandboxPlan,
        fixture: &SandboxConformanceFixture,
        outcome: SessionOutcome,
    ) -> Result<SandboxConformanceObservation, ()> {
        let admission =
            admit_backend(plan, self.descriptor.descriptor(), AdmissionProfile::Conformance)
                .map_err(|_| ())?;
        let projected = self.project(plan, &admission, fixture.secret_canary())?;
        let mut session = MacosSession::new(
            projected.launch,
            projected.manifest,
            projected.helper_digest,
            projected.proxy_digest,
            64,
            crate::session::SessionResources::new(
                projected.exec_status,
                projected.proxy,
                projected.secrets,
            ),
        )
        .map_err(|_| ())?;
        session
            .record_activation(ProcessTreeIdentity::new(1701, Some(19), Some(1701), true))
            .map_err(|_| ())?;
        if outcome.cancellation {
            session.record_cancellation(CancellationReason::User).map_err(|_| ())?;
        }
        session.record_termination(&OsExitObservation::Code(0)).map_err(|_| ())?;
        let release = session.record_release().map_err(|_| ())?;
        let observations = session.observations();
        let observed_plan = observations
            .first()
            .map_or([0; 32], |observation| *observation.plan_digest().as_bytes());
        let activation_count = observations
            .iter()
            .filter(|observation| observation.kind() == ObservationKind::Activated)
            .count() as u64;
        let mut ordinary = session.manifest().canonical_bytes().to_vec();
        ordinary.extend_from_slice(format!("{:?}", session.native_observations()).as_bytes());
        Ok(SandboxConformanceObservation::new(
            outcome.decision,
            SandboxLifecyclePhase::Released,
            Vec::new(),
            outcome.resource_observed,
            fixture.resource_limit(),
            activation_count,
            0,
            outcome.cancellation,
            release.cleanup().is_complete(),
            *plan.digest().as_bytes(),
            observed_plan,
            observations.iter().map(|observation| observation.sequence()).collect(),
            ordinary,
            outcome.process_tree_contained,
            outcome.terminal_controlled,
        ))
    }
}

fn prepare_proxy(
    plan: &CheckedSandboxPlan,
) -> Result<(Option<ManagedProxy>, Option<ProtectedProxyRoute>), ()> {
    if plan.requirements().network().is_empty() {
        return Ok((None, None));
    }
    let options = RuntimeNetworkOptions::new(
        DnsMode::ProxySystem,
        RedirectMode::Deny,
        ProxyMode::HttpConnect,
        NetworkBounds::new(4, 2, 64 * 1024, 128 * 1024, 5_000, 15_000, 64, 16_384)
            .map_err(|_| ())?,
        Vec::new(),
    );
    let token_bytes = [31_u8; 32];
    let proxy = ManagedProxy::start(
        NetworkPlan::from_checked(plan, options).map_err(|_| ())?,
        RoutingToken::new(token_bytes),
    )
    .map_err(|_| ())?;
    let handle =
        NativeProtectedHandle::from_bytes("peritus-macos-conformance-proxy", token_bytes.to_vec())
            .map_err(|_| ())?;
    let protected =
        ProtectedProxyRoute::new(proxy.endpoint().socket_addr(), handle).map_err(|_| ())?;
    Ok((Some(proxy), Some(protected)))
}

pub(super) struct PreparedProjection {
    launch: NativeLaunchDescription,
    pub(super) manifest: HelperManifest,
    helper_digest: Sha256Digest,
    proxy_digest: Option<Sha256Digest>,
    exec_status: crate::exec_status::ExecStatusOwner,
    proxy: Option<ManagedProxy>,
    secrets: SecretDeliverySession,
}

impl SandboxConformanceSubject for MacosConformanceSubject {
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
