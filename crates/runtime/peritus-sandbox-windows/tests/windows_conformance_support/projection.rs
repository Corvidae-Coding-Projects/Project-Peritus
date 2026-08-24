use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use peritus_process::CommandSpec;
use peritus_sandbox::{AdmissionProfile, CheckedSandboxPlan, admit_backend};
use peritus_sandbox_windows::{
    AppContainerProfile, EnvironmentEntry, HelperManifest, InheritedHandlePolicy, JobPlan,
    NetworkIsolation, ObservationBinding, ObservationStatus, PathPolicy, ProcessPolicy,
    ProtectedSecretHandle, ProxyRoute, ResourceControlPlan, RuntimeIdentity,
    SecretHandleDestination, TerminalMapping, TokenProfile, WindowsBackendDescriptor, WindowsError,
    WindowsObservation, WindowsPath, WindowsPhase, WindowsProbe, WindowsRecoveryRecord,
    compile_acl_plan, managed_wfp_policy_digest, production_resource_levels,
    secret_reference_digest,
};
use peritus_types::Sha256Digest;

const HELPER_DIGEST: Sha256Digest = Sha256Digest::new([0xC3; 32]);
const CONTROLLER_DIGEST: Sha256Digest = Sha256Digest::new([0x77; 32]);

pub struct ProjectedSession {
    manifest: HelperManifest,
    recovery: WindowsRecoveryRecord,
    binding: ObservationBinding,
    phase: WindowsPhase,
    observations: Vec<WindowsObservation>,
    cancellation: bool,
}

impl ProjectedSession {
    pub fn prepare(plan: &CheckedSandboxPlan) -> Result<Self, WindowsError> {
        let managed = !plan.requirements().network().is_empty();
        let mut evidence = peritus_sandbox_windows::ProbeEvidence::supported_fixture();
        evidence.helper_digest = Some(HELPER_DIGEST);
        evidence.managed_network = managed;
        let probe = WindowsProbe::from_evidence(evidence)?;
        let controller = managed.then_some(CONTROLLER_DIGEST);
        let descriptor = WindowsBackendDescriptor::from_probe(probe, controller)?;
        let admission = admit_backend(plan, descriptor.common(), AdmissionProfile::Production)
            .map_err(|_| {
                projection_error("Windows conformance admission rejected a checked plan")
            })?;
        let token = token()?;
        let path_policy = PathPolicy::new(WindowsPath::new(r"C:\workspace")?, Vec::new())?;
        let acl = compile_acl_plan(plan, &path_policy, token.principal_sid())?;
        let environment = plan
            .requirements()
            .environment()
            .literal_names()
            .iter()
            .map(|name| EnvironmentEntry::new(name.as_str(), "conformance"))
            .collect::<Result<Vec<_>, _>>()?;
        let secrets = secret_handles(plan)?;
        let inherited = InheritedHandlePolicy::new(
            secrets
                .iter()
                .filter(|value| matches!(value.destination(), SecretHandleDestination::Brokered(_)))
                .map(ProtectedSecretHandle::handle)
                .collect(),
        )?;
        let network = network(plan, &token, controller)?;
        let job = JobPlan::from_checked_plan(plan);
        let resources = ResourceControlPlan::from_checked_plan(plan, production_resource_levels());
        let command = CommandSpec::new(
            plan.requirements().process().program().as_str(),
            Vec::<String>::new(),
        )
        .map_err(|_| projection_error("Windows conformance command cannot be represented"))?;
        let manifest = HelperManifest::build(
            plan.binding().process_id(),
            plan,
            &admission,
            HELPER_DIGEST,
            &acl,
            token,
            &command,
            WindowsPath::new(r"C:\workspace")?,
            environment,
            job,
            ProcessPolicy::from_checked_plan(plan),
            TerminalMapping::from_checked_plan(plan)?,
            resources,
            network,
            secrets,
            inherited,
        )?;
        let binding = ObservationBinding::new(
            plan.digest(),
            descriptor.common().digest(),
            descriptor.probe().digest(),
            admission.preparation_digest(),
        );
        let identity = RuntimeIdentity::new(
            plan.binding().process_id(),
            admission.preparation_digest(),
            HELPER_DIGEST,
            manifest.digest(),
            peritus_codec::sha256(token_sid(&manifest).as_bytes()),
            acl.digest(),
        );
        let observations = vec![observation(1, binding, WindowsPhase::Prepared)];
        Ok(Self {
            manifest,
            recovery: WindowsRecoveryRecord::prepared(identity),
            binding,
            phase: WindowsPhase::Prepared,
            observations,
            cancellation: false,
        })
    }

    pub fn activate(&mut self) -> Result<(), WindowsError> {
        self.advance(WindowsPhase::Activated, false, false, false)
    }

    pub fn cancel(&mut self) -> Result<(), WindowsError> {
        self.cancellation = true;
        self.advance(WindowsPhase::CancelRequested, false, false, false)
    }

    pub fn terminate(&mut self) -> Result<(), WindowsError> {
        self.advance(WindowsPhase::Terminated, false, false, true)
    }

    pub fn release(&mut self) -> Result<(), WindowsError> {
        self.advance(WindowsPhase::Released, true, true, true)
    }

    pub const fn manifest(&self) -> &HelperManifest {
        &self.manifest
    }

    pub const fn phase(&self) -> WindowsPhase {
        self.phase
    }

    pub fn observations(&self) -> &[WindowsObservation] {
        &self.observations
    }

    pub const fn cancellation(&self) -> bool {
        self.cancellation
    }

    pub const fn cleanup_complete(&self) -> bool {
        self.recovery.cleanup_complete()
    }

    fn advance(
        &mut self,
        phase: WindowsPhase,
        acl: bool,
        secrets: bool,
        reaped: bool,
    ) -> Result<(), WindowsError> {
        self.recovery.advance(phase, acl, secrets, reaped)?;
        let sequence = u64::try_from(self.observations.len() + 1).unwrap_or(u64::MAX);
        self.observations.push(observation(sequence, self.binding, phase));
        self.phase = phase;
        Ok(())
    }
}

fn network(
    plan: &CheckedSandboxPlan,
    token: &TokenProfile,
    controller: Option<Sha256Digest>,
) -> Result<NetworkIsolation, WindowsError> {
    let Some(controller) = controller else {
        return Ok(NetworkIsolation::DenyAll);
    };
    let endpoint = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 43_443);
    let digest =
        managed_wfp_policy_digest(controller, token.principal_sid(), endpoint, plan.digest());
    Ok(NetworkIsolation::ManagedProxy(ProxyRoute::new(endpoint, 0xC3_00, plan.digest(), digest)?))
}

fn secret_handles(plan: &CheckedSandboxPlan) -> Result<Vec<ProtectedSecretHandle>, WindowsError> {
    plan.requirements()
        .secrets()
        .iter()
        .enumerate()
        .map(|(index, requirement)| {
            ProtectedSecretHandle::new(
                0xD0_00 + u64::try_from(index).unwrap_or(u64::MAX),
                secret_reference_digest(requirement.reference()),
                SecretHandleDestination::from(requirement.delivery()),
            )
        })
        .collect()
}

fn token() -> Result<TokenProfile, WindowsError> {
    Ok(TokenProfile::AppContainer(AppContainerProfile::new("Peritus.Conformance", "S-1-15-2-123")?))
}

fn token_sid(manifest: &HelperManifest) -> &str {
    manifest.token().principal_sid()
}

const fn observation(
    sequence: u64,
    binding: ObservationBinding,
    phase: WindowsPhase,
) -> WindowsObservation {
    WindowsObservation::new(sequence, binding, phase, None, None, None, ObservationStatus::Verified)
}

fn projection_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        peritus_sandbox_windows::WindowsErrorKind::PreparationMismatch,
        peritus_sandbox_windows::WindowsOperation::Prepare,
        peritus_sandbox_windows::WindowsRecovery::Replan,
        detail,
    )
}
