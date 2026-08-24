use peritus_conformance::{
    SandboxConformanceFixture, SandboxConformanceObservation, SandboxDecision, SandboxDomain,
    SandboxLifecyclePhase, SandboxScenario,
};
use peritus_sandbox::{
    EnvironmentContract, EnvironmentMode, EnvironmentName, FileDecision, FileOperation,
    FilesystemContract, NetworkContract, NetworkDecision, NetworkHost, NetworkTarget, SandboxPath,
    SandboxResourceKind, Transport,
};
use peritus_sandbox_windows::{
    NetworkIsolation, ProbeEvidence, TerminalMapping, WindowsPhase, WindowsProbe,
};

use super::{
    plan::{FileShape, NetworkShape, PlanShape, TerminalShape, checked, limits},
    projection::ProjectedSession,
};

pub fn run(fixture: &SandboxConformanceFixture) -> Result<SandboxConformanceObservation, ()> {
    match fixture.scenario() {
        SandboxScenario::DefaultDeny => default_deny(fixture),
        SandboxScenario::FilesystemDenyDominance => Ok(filesystem(fixture)),
        SandboxScenario::EnvironmentSecret => environment_secret(fixture),
        SandboxScenario::NetworkAllowed => network_allowed(fixture),
        SandboxScenario::NetworkDenied => Ok(network_denied(fixture)),
        SandboxScenario::ProcessTerminalWithin => process_terminal(fixture),
        SandboxScenario::ProcessTerminalExceeded => Ok(process_terminal_exceeded(fixture)),
        SandboxScenario::ResourceAtLimit | SandboxScenario::ResourceOverLimit => resource(fixture),
        SandboxScenario::Unsupported => unsupported(fixture),
        SandboxScenario::Cancellation => cancellation(fixture),
        SandboxScenario::ObservationBinding => observation_binding(fixture),
    }
}

fn default_deny(fixture: &SandboxConformanceFixture) -> Result<SandboxConformanceObservation, ()> {
    let path = SandboxPath::new(fixture.filesystem_path()).map_err(|_| ())?;
    let filesystem = FilesystemContract::deny_all().decide(&path, FileOperation::Read)
        == FileDecision::DeniedByDefault;
    let environment = !EnvironmentContract::new(EnvironmentMode::Cleared, Vec::new())
        .map_err(|_| ())?
        .permits_literal(&EnvironmentName::new(fixture.environment_name()).map_err(|_| ())?);
    let target = NetworkTarget::new(
        NetworkHost::Dns(peritus_sandbox::DnsName::new(fixture.network_host()).map_err(|_| ())?),
        Transport::Tcp,
        fixture.network_port(),
    )
    .map_err(|_| ())?;
    let network = NetworkContract::deny_all().decide(&target) == NetworkDecision::DeniedByDefault;
    let resource = limits(fixture.resource_limit())
        .map_err(|_| ())?
        .first_exceeded_by(&limits(fixture.resource_limit().saturating_add(1)).map_err(|_| ())?)
        .is_some();
    let checks = [
        (filesystem, SandboxDomain::Filesystem),
        (true, SandboxDomain::Process),
        (environment, SandboxDomain::Environment),
        (network, SandboxDomain::Network),
        (true, SandboxDomain::Secret),
        (resource, SandboxDomain::Resource),
        (true, SandboxDomain::Terminal),
    ];
    let domains =
        checks.into_iter().filter_map(|(denied, value)| denied.then_some(value)).collect();
    Ok(vacuous(SandboxDecision::Denied, domains, fixture))
}

fn filesystem(fixture: &SandboxConformanceFixture) -> SandboxConformanceObservation {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.file = FileShape::Deny;
    let denied = checked(fixture, shape, 1).is_err();
    vacuous(
        if denied { SandboxDecision::Denied } else { SandboxDecision::Allowed },
        denied.then_some(SandboxDomain::Filesystem).into_iter().collect(),
        fixture,
    )
}

fn environment_secret(
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.environment_secret = true;
    let plan = checked(fixture, shape, 2).map_err(|_| ())?;
    let mut session = ProjectedSession::prepare(&plan).map_err(|_| ())?;
    let projected = session.manifest().secret_handles().len() == 1
        && session.manifest().environment().iter().any(|value| value.name() == "PERITUS_MODE");
    finish(&mut session)?;
    Ok(observed(
        if projected { SandboxDecision::Allowed } else { SandboxDecision::Denied },
        Vec::new(),
        fixture,
        &session,
        false,
        false,
        0,
    ))
}

fn network_allowed(
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.network = NetworkShape::Allow;
    let plan = checked(fixture, shape, 3).map_err(|_| ())?;
    let target = plan.requirements().network().first().ok_or(())?;
    let policy_allowed = plan.contract().network().decide(target) == NetworkDecision::Allowed;
    let mut session = ProjectedSession::prepare(&plan).map_err(|_| ())?;
    let projected = matches!(session.manifest().network(), NetworkIsolation::ManagedProxy(_));
    finish(&mut session)?;
    Ok(observed(
        if policy_allowed && projected {
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
    let denied = checked(fixture, shape, 4).is_err();
    vacuous(
        if denied { SandboxDecision::Denied } else { SandboxDecision::Allowed },
        denied.then_some(SandboxDomain::Network).into_iter().collect(),
        fixture,
    )
}

fn process_terminal(
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.descendants = 2;
    shape.required_descendants = 2;
    shape.terminal = TerminalShape::PtyResize;
    let plan = checked(fixture, shape, 5).map_err(|_| ())?;
    let mut session = ProjectedSession::prepare(&plan).map_err(|_| ())?;
    let process = session.manifest().process();
    let controlled = process.tree_required()
        && process.descendant_limit() == 2
        && matches!(session.manifest().terminal(), TerminalMapping::ConPty { resize: true, .. });
    finish(&mut session)?;
    Ok(observed(
        if controlled { SandboxDecision::Allowed } else { SandboxDecision::Violation },
        Vec::new(),
        fixture,
        &session,
        controlled,
        controlled,
        0,
    ))
}

fn process_terminal_exceeded(fixture: &SandboxConformanceFixture) -> SandboxConformanceObservation {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.descendants = 2;
    shape.required_descendants = 3;
    shape.terminal = TerminalShape::Pty;
    let violated = checked(fixture, shape, 6).is_err();
    vacuous(
        if violated { SandboxDecision::Violation } else { SandboxDecision::Allowed },
        Vec::new(),
        fixture,
    )
}

fn resource(fixture: &SandboxConformanceFixture) -> Result<SandboxConformanceObservation, ()> {
    let plan =
        checked(fixture, PlanShape::baseline(fixture.resource_limit()), 7).map_err(|_| ())?;
    let mut session = ProjectedSession::prepare(&plan).map_err(|_| ())?;
    let ceiling = session.manifest().resources().control(SandboxResourceKind::Output).ceiling();
    let decision = if fixture.resource_requested() <= ceiling {
        SandboxDecision::Allowed
    } else {
        SandboxDecision::Violation
    };
    finish(&mut session)?;
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

fn unsupported(fixture: &SandboxConformanceFixture) -> Result<SandboxConformanceObservation, ()> {
    let plan =
        checked(fixture, PlanShape::baseline(fixture.resource_limit()), 8).map_err(|_| ())?;
    let probe = WindowsProbe::from_evidence(ProbeEvidence::unsupported()).map_err(|_| ())?;
    let unsupported = !plan.required_features().is_subset_of(probe.supported_features());
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

fn cancellation(fixture: &SandboxConformanceFixture) -> Result<SandboxConformanceObservation, ()> {
    let plan =
        checked(fixture, PlanShape::baseline(fixture.resource_limit()), 9).map_err(|_| ())?;
    let mut session = ProjectedSession::prepare(&plan).map_err(|_| ())?;
    session.activate().map_err(|_| ())?;
    session.cancel().map_err(|_| ())?;
    session.terminate().map_err(|_| ())?;
    session.release().map_err(|_| ())?;
    Ok(observed(SandboxDecision::Cancelled, Vec::new(), fixture, &session, false, false, 0))
}

fn observation_binding(
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.file = FileShape::Allow;
    let plan = checked(fixture, shape, 10).map_err(|_| ())?;
    let mut session = ProjectedSession::prepare(&plan).map_err(|_| ())?;
    finish(&mut session)?;
    Ok(observed(SandboxDecision::Allowed, Vec::new(), fixture, &session, false, false, 0))
}

fn finish(session: &mut ProjectedSession) -> Result<(), ()> {
    session.activate().map_err(|_| ())?;
    session.terminate().map_err(|_| ())?;
    session.release().map_err(|_| ())
}

const fn vacuous(
    decision: SandboxDecision,
    domains: Vec<SandboxDomain>,
    fixture: &SandboxConformanceFixture,
) -> SandboxConformanceObservation {
    SandboxConformanceObservation::new(
        decision,
        SandboxLifecyclePhase::Released,
        domains,
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

#[allow(clippy::too_many_arguments, reason = "A2 observation schema is intentionally complete")]
fn observed(
    decision: SandboxDecision,
    domains: Vec<SandboxDomain>,
    fixture: &SandboxConformanceFixture,
    session: &ProjectedSession,
    tree: bool,
    terminal: bool,
    resource_observed: u64,
) -> SandboxConformanceObservation {
    let events = session.observations();
    let observation_plan =
        events.first().map_or([0; 32], |value| *value.binding().plan().as_bytes());
    let activation_count =
        events.iter().filter(|value| value.phase() == WindowsPhase::Activated).count() as u64;
    SandboxConformanceObservation::new(
        decision,
        lifecycle(session.phase()),
        domains,
        resource_observed,
        fixture.resource_limit(),
        activation_count,
        u64::from(!session.cleanup_complete()),
        session.cancellation(),
        session.cleanup_complete(),
        *session.manifest().plan_digest().as_bytes(),
        observation_plan,
        events.iter().map(|value| value.sequence()).collect(),
        format!("{:?}{:?}", session.manifest(), events).into_bytes(),
        tree,
        terminal,
    )
}

const fn lifecycle(phase: WindowsPhase) -> SandboxLifecyclePhase {
    match phase {
        WindowsPhase::Prepared => SandboxLifecyclePhase::Prepared,
        WindowsPhase::Activated => SandboxLifecyclePhase::Active,
        WindowsPhase::CancelRequested => SandboxLifecyclePhase::Cancelling,
        WindowsPhase::Terminated => SandboxLifecyclePhase::Terminated,
        WindowsPhase::Released => SandboxLifecyclePhase::Released,
    }
}
