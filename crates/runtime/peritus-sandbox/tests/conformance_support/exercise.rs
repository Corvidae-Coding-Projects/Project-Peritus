//! A2 behavior cases executed through real contracts, plans, admission, and sessions.

use super::plan_fixture::{
    FileShape, NetworkShape, PlanShape, TerminalShape, checked_plan, limits,
};
use peritus_conformance::{
    SandboxConformanceFixture, SandboxConformanceObservation, SandboxDecision, SandboxDomain,
    SandboxLifecyclePhase, SandboxScenario,
};
use peritus_sandbox::{
    AdmissionProfile, BackendDescriptor, BackendKind, BackendName, BackendVersion,
    CancellationAcceptance, CancellationReason, DescendantPolicy, EnvironmentContract,
    EnvironmentMode, EnvironmentName, FileDecision, FileOperation, FilesystemContract,
    InputPermission, NetworkContract, NetworkDecision, NetworkHost, PathSemantics, ProbeDecision,
    ReferenceBackend, ReferenceProbe, ReferenceSession, RequestedTerminalOperation,
    ResizePermission, ResourceDecision, ResourceFidelity, SandboxError, SandboxPath, SandboxPhase,
    SandboxPreparation, SandboxResourceKind, SecretContract, SignalPolicy, TerminalContract,
    TerminalMode, TerminalModes, TerminalSignalPermission, TerminalSize, TerminationKind,
    Transport, TreeContainment, admit_backend,
};
use peritus_types::ResourceQuantity;

pub(super) fn exercise(
    backend: &ReferenceBackend,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, SandboxError> {
    match fixture.scenario() {
        SandboxScenario::DefaultDeny => default_deny(fixture),
        SandboxScenario::FilesystemDenyDominance => Ok(filesystem_denied(fixture)),
        SandboxScenario::EnvironmentSecret => environment_secret(backend, fixture),
        SandboxScenario::NetworkAllowed => network_allowed(backend, fixture),
        SandboxScenario::NetworkDenied => Ok(network_denied(fixture)),
        SandboxScenario::ProcessTerminalWithin => process_terminal(backend, fixture),
        SandboxScenario::ProcessTerminalExceeded => Ok(process_terminal_exceeded(fixture)),
        SandboxScenario::ResourceAtLimit | SandboxScenario::ResourceOverLimit => {
            resource_boundary(backend, fixture)
        }
        SandboxScenario::Unsupported => unsupported(fixture),
        SandboxScenario::Cancellation => cancellation(backend, fixture),
        SandboxScenario::ObservationBinding => observation_binding(backend, fixture),
    }
}

fn default_deny(
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, SandboxError> {
    let path = SandboxPath::new(fixture.filesystem_path())?;
    let filesystem_denied = FilesystemContract::deny_all().decide(&path, FileOperation::Read)
        == FileDecision::DeniedByDefault;
    let allowed_program = SandboxPath::new("/bin/allowed")?;
    let process = peritus_sandbox::ProcessContract::new(
        vec![allowed_program],
        DescendantPolicy::Denied,
        SignalPolicy::Denied,
        TreeContainment::Required,
        1,
    )?;
    let process_denied = !process.root_programs().contains(&path);
    let environment_denied = !EnvironmentContract::new(EnvironmentMode::Cleared, Vec::new())?
        .permits_literal(&EnvironmentName::new(fixture.environment_name())?);
    let target = peritus_sandbox::NetworkTarget::new(
        NetworkHost::Dns(peritus_sandbox::DnsName::new(fixture.network_host())?),
        Transport::Tcp,
        fixture.network_port(),
    )?;
    let network_denied =
        NetworkContract::deny_all().decide(&target) == NetworkDecision::DeniedByDefault;
    let secret_denied = SecretContract::deny_all().grants().is_empty();
    let resource_denied = limits(fixture.resource_limit())?
        .first_exceeded_by(&limits(fixture.resource_limit().saturating_add(1))?)
        .is_some();
    let terminal_denied = !TerminalContract::new(
        TerminalModes::empty(),
        InputPermission::Denied,
        ResizePermission::Denied,
        TerminalSignalPermission::Denied,
        peritus_sandbox::TerminalLimits::new(
            None,
            ResourceQuantity::new(5),
            ResourceQuantity::new(fixture.resource_limit()),
        )?,
    )?
    .modes()
    .contains(TerminalMode::Pty);
    let checks = [
        (filesystem_denied, SandboxDomain::Filesystem),
        (process_denied, SandboxDomain::Process),
        (environment_denied, SandboxDomain::Environment),
        (network_denied, SandboxDomain::Network),
        (secret_denied, SandboxDomain::Secret),
        (resource_denied, SandboxDomain::Resource),
        (terminal_denied, SandboxDomain::Terminal),
    ];
    let denied_domains =
        checks.into_iter().filter_map(|(denied, domain)| denied.then_some(domain)).collect();
    Ok(vacuous(SandboxDecision::Denied, denied_domains, fixture))
}

fn filesystem_denied(fixture: &SandboxConformanceFixture) -> SandboxConformanceObservation {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.filesystem = FileShape::Deny;
    let denied = checked_plan(fixture, shape, 1).is_err();
    vacuous(
        if denied { SandboxDecision::Denied } else { SandboxDecision::Allowed },
        if denied { vec![SandboxDomain::Filesystem] } else { Vec::new() },
        fixture,
    )
}

fn environment_secret(
    backend: &ReferenceBackend,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, SandboxError> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.environment_secret = true;
    let plan = checked_plan(fixture, shape, 2)?;
    let mut session = session(backend, &plan)?;
    let environment = session.evaluate(&ReferenceProbe::LiteralEnvironment(
        EnvironmentName::new(fixture.environment_name())?,
    ))?;
    let secret = plan.requirements().secrets().first().expect("fixture requires a secret").clone();
    let secret = session.evaluate(&ReferenceProbe::Secret(secret))?;
    finish(&mut session)?;
    let decision = if environment == ProbeDecision::Allowed && secret == ProbeDecision::Allowed {
        SandboxDecision::Allowed
    } else {
        SandboxDecision::Denied
    };
    Ok(observed(decision, Vec::new(), fixture, &session, false, false, 0))
}

fn network_allowed(
    backend: &ReferenceBackend,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, SandboxError> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.network = NetworkShape::Allow;
    let plan = checked_plan(fixture, shape, 3)?;
    let target = plan.requirements().network().first().expect("fixture requires network").clone();
    let mut session = session(backend, &plan)?;
    let decision = session.evaluate(&ReferenceProbe::Network(target))?;
    finish(&mut session)?;
    Ok(observed(
        if decision == ProbeDecision::Allowed {
            SandboxDecision::Allowed
        } else {
            SandboxDecision::Denied
        },
        Vec::new(),
        fixture,
        &session,
        false,
        false,
        0,
    ))
}

fn network_denied(fixture: &SandboxConformanceFixture) -> SandboxConformanceObservation {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.network = NetworkShape::Deny;
    let denied = checked_plan(fixture, shape, 4).is_err();
    vacuous(
        if denied { SandboxDecision::Denied } else { SandboxDecision::Allowed },
        if denied { vec![SandboxDomain::Network] } else { Vec::new() },
        fixture,
    )
}

fn process_terminal(
    backend: &ReferenceBackend,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, SandboxError> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.descendant_limit = 2;
    shape.descendant_required = 2;
    shape.terminal = TerminalShape::PtyResize;
    let plan = checked_plan(fixture, shape, 5)?;
    let mut session = session(backend, &plan)?;
    let descendants = session.evaluate(&ReferenceProbe::DescendantCount(2))?;
    let terminal = session.evaluate(&ReferenceProbe::Terminal(
        RequestedTerminalOperation::Resize(TerminalSize::new(100, 30)?),
    ))?;
    finish(&mut session)?;
    let allowed = descendants == ProbeDecision::Allowed && terminal == ProbeDecision::Allowed;
    Ok(observed(
        if allowed { SandboxDecision::Allowed } else { SandboxDecision::Violation },
        Vec::new(),
        fixture,
        &session,
        allowed,
        allowed,
        0,
    ))
}

fn process_terminal_exceeded(fixture: &SandboxConformanceFixture) -> SandboxConformanceObservation {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.descendant_limit = 2;
    shape.descendant_required = 3;
    shape.terminal = TerminalShape::Pty;
    let violated = checked_plan(fixture, shape, 6).is_err();
    vacuous(
        if violated { SandboxDecision::Violation } else { SandboxDecision::Allowed },
        Vec::new(),
        fixture,
    )
}

fn resource_boundary(
    backend: &ReferenceBackend,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, SandboxError> {
    let plan = checked_plan(fixture, PlanShape::baseline(fixture.resource_limit()), 7)?;
    let mut session = session(backend, &plan)?;
    let decision = session
        .charge(SandboxResourceKind::Output, ResourceQuantity::new(fixture.resource_requested()))?;
    finish(&mut session)?;
    let decision = match decision {
        ResourceDecision::WithinLimit => SandboxDecision::Allowed,
        ResourceDecision::LimitExceeded(_) => SandboxDecision::Violation,
    };
    Ok(observed(
        decision,
        Vec::new(),
        fixture,
        &session,
        false,
        false,
        fixture.resource_requested(),
    ))
}

fn unsupported(
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, SandboxError> {
    let plan = checked_plan(fixture, PlanShape::baseline(fixture.resource_limit()), 8)?;
    let descriptor = BackendDescriptor::new(
        BackendName::new("unsupported-conformance")?,
        BackendVersion::new("1")?,
        BackendKind::Native,
        PathSemantics::LogicalUtf8,
        ResourceFidelity::Hard,
        peritus_sandbox::FeatureSet::empty(),
    );
    let unsupported = admit_backend(&plan, &descriptor, AdmissionProfile::Conformance).is_err();
    Ok(SandboxConformanceObservation::new(
        if unsupported { SandboxDecision::Unsupported } else { SandboxDecision::Allowed },
        SandboxLifecyclePhase::Planned,
        Vec::new(),
        0,
        fixture.resource_limit(),
        0,
        0,
        false,
        false,
        *plan.digest().as_bytes(),
        [0; 32],
        Vec::new(),
        Vec::new(),
        false,
        false,
    ))
}

fn cancellation(
    backend: &ReferenceBackend,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, SandboxError> {
    let plan = checked_plan(fixture, PlanShape::baseline(fixture.resource_limit()), 9)?;
    let mut session = session(backend, &plan)?;
    let accepted = session.cancel(CancellationReason::Requested)?;
    session.terminate(TerminationKind::Cancelled(CancellationReason::Requested))?;
    session.release()?;
    let decision = if accepted == CancellationAcceptance::Accepted {
        SandboxDecision::Cancelled
    } else {
        SandboxDecision::Violation
    };
    Ok(observed(decision, Vec::new(), fixture, &session, false, false, 0))
}

fn observation_binding(
    backend: &ReferenceBackend,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, SandboxError> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.filesystem = FileShape::Allow;
    let plan = checked_plan(fixture, shape, 10)?;
    let requirement = plan.requirements().files().first().expect("fixture requires a file").clone();
    let mut session = session(backend, &plan)?;
    let decision = session.evaluate(&ReferenceProbe::Filesystem(requirement))?;
    finish(&mut session)?;
    Ok(observed(
        if decision == ProbeDecision::Allowed {
            SandboxDecision::Allowed
        } else {
            SandboxDecision::Denied
        },
        Vec::new(),
        fixture,
        &session,
        false,
        false,
        0,
    ))
}

fn session(
    backend: &ReferenceBackend,
    plan: &peritus_sandbox::CheckedSandboxPlan,
) -> Result<ReferenceSession, SandboxError> {
    let admission = backend.admit(plan, AdmissionProfile::Conformance)?;
    let mut session = backend.prepare(plan, &admission)?;
    session.activate()?;
    Ok(session)
}

fn finish(session: &mut ReferenceSession) -> Result<(), SandboxError> {
    session.terminate(TerminationKind::Exited(0))?;
    session.release()
}

const fn vacuous(
    decision: SandboxDecision,
    denied_domains: Vec<SandboxDomain>,
    fixture: &SandboxConformanceFixture,
) -> SandboxConformanceObservation {
    SandboxConformanceObservation::new(
        decision,
        SandboxLifecyclePhase::Released,
        denied_domains,
        fixture.resource_requested(),
        fixture.resource_limit(),
        0,
        0,
        false,
        true,
        [0; 32],
        [0; 32],
        Vec::new(),
        Vec::new(),
        false,
        false,
    )
}

#[allow(clippy::too_many_arguments, reason = "A2 observation fields are independently asserted")]
fn observed(
    decision: SandboxDecision,
    denied_domains: Vec<SandboxDomain>,
    fixture: &SandboxConformanceFixture,
    session: &ReferenceSession,
    process_tree_contained: bool,
    terminal_controlled: bool,
    resource_observed: u64,
) -> SandboxConformanceObservation {
    let observations = session.observations();
    let observation_plan_digest =
        observations.first().map_or([0; 32], |event| *event.plan_digest().as_bytes());
    let activation_count = observations
        .iter()
        .filter(|event| event.kind() == peritus_sandbox::ObservationKind::Activated)
        .count() as u64;
    SandboxConformanceObservation::new(
        decision,
        lifecycle(session.phase()),
        denied_domains,
        resource_observed,
        fixture.resource_limit(),
        activation_count,
        u64::from(matches!(session.phase(), SandboxPhase::Active | SandboxPhase::Cancelling)),
        session.cancellation().is_cancelled(),
        session.teardown_completeness() == peritus_sandbox::TeardownCompleteness::Complete,
        *session.plan().digest().as_bytes(),
        observation_plan_digest,
        observations.iter().map(|event| event.sequence()).collect(),
        format!("{observations:?}").into_bytes(),
        process_tree_contained,
        terminal_controlled,
    )
}

const fn lifecycle(phase: SandboxPhase) -> SandboxLifecyclePhase {
    match phase {
        SandboxPhase::Planned => SandboxLifecyclePhase::Planned,
        SandboxPhase::Prepared => SandboxLifecyclePhase::Prepared,
        SandboxPhase::Active => SandboxLifecyclePhase::Active,
        SandboxPhase::Cancelling => SandboxLifecyclePhase::Cancelling,
        SandboxPhase::Terminated => SandboxLifecyclePhase::Terminated,
        SandboxPhase::Released => SandboxLifecyclePhase::Released,
    }
}
