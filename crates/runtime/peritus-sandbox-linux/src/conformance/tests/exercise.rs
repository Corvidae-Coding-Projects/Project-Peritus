//! A2 cases executed through Linux checked plans, projections, manifests, and sessions.

use super::{
    adapter::{LinuxConformanceSubject, SessionOutcome},
    fixture::{FileShape, NetworkShape, PlanShape, TerminalShape, checked_plan},
};
use crate::ResourcePlan;
use peritus_conformance::{
    SandboxConformanceFixture, SandboxConformanceObservation, SandboxDecision, SandboxDomain,
    SandboxLifecyclePhase, SandboxScenario,
};
use peritus_sandbox::{
    AdmissionProfile, BackendDescriptor, BackendKind, BackendName, BackendVersion, PathSemantics,
    ResourceFidelity, admit_backend,
};

pub(super) fn exercise(
    subject: &LinuxConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    match fixture.scenario() {
        SandboxScenario::DefaultDeny => Ok(vacuous(
            SandboxDecision::Denied,
            vec![
                SandboxDomain::Filesystem,
                SandboxDomain::Process,
                SandboxDomain::Environment,
                SandboxDomain::Network,
                SandboxDomain::Secret,
                SandboxDomain::Resource,
                SandboxDomain::Terminal,
            ],
            fixture,
        )),
        SandboxScenario::FilesystemDenyDominance => Ok(filesystem_denied(subject, fixture)),
        SandboxScenario::EnvironmentSecret => environment_secret(subject, fixture),
        SandboxScenario::NetworkAllowed => network_allowed(subject, fixture),
        SandboxScenario::NetworkDenied => Ok(network_denied(subject, fixture)),
        SandboxScenario::ProcessTerminalWithin => process_terminal(subject, fixture),
        SandboxScenario::ProcessTerminalExceeded => Ok(process_terminal_exceeded(subject, fixture)),
        SandboxScenario::ResourceAtLimit | SandboxScenario::ResourceOverLimit => {
            resource_boundary(subject, fixture)
        }
        SandboxScenario::Unsupported => unsupported(subject, fixture),
        SandboxScenario::Cancellation => cancellation(subject, fixture),
        SandboxScenario::ObservationBinding => observation_binding(subject, fixture),
    }
}

fn filesystem_denied(
    subject: &LinuxConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> SandboxConformanceObservation {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.filesystem = FileShape::Deny;
    let denied = checked_plan(fixture, subject.workspace(), shape, 1).is_err();
    vacuous(
        if denied { SandboxDecision::Denied } else { SandboxDecision::Allowed },
        if denied { vec![SandboxDomain::Filesystem] } else { Vec::new() },
        fixture,
    )
}

fn environment_secret(
    subject: &LinuxConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.environment_secret = true;
    let plan = checked_plan(fixture, subject.workspace(), shape, 2).map_err(|_| ())?;
    subject.run_session(
        &plan,
        fixture,
        SessionOutcome::new(SandboxDecision::Allowed, false, false, false, 0),
    )
}

fn network_allowed(
    subject: &LinuxConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.network = NetworkShape::Allow;
    let plan = checked_plan(fixture, subject.workspace(), shape, 3).map_err(|_| ())?;
    subject.run_session(
        &plan,
        fixture,
        SessionOutcome::new(SandboxDecision::Allowed, false, false, false, 0),
    )
}

fn network_denied(
    subject: &LinuxConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> SandboxConformanceObservation {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.network = NetworkShape::Deny;
    let denied = checked_plan(fixture, subject.workspace(), shape, 4).is_err();
    vacuous(
        if denied { SandboxDecision::Denied } else { SandboxDecision::Allowed },
        if denied { vec![SandboxDomain::Network] } else { Vec::new() },
        fixture,
    )
}

fn process_terminal(
    subject: &LinuxConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.descendant_limit = 2;
    shape.descendant_required = 2;
    shape.terminal = TerminalShape::PtyResize;
    let plan = checked_plan(fixture, subject.workspace(), shape, 5).map_err(|_| ())?;
    subject.run_session(
        &plan,
        fixture,
        SessionOutcome::new(SandboxDecision::Allowed, false, true, true, 0),
    )
}

fn process_terminal_exceeded(
    subject: &LinuxConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> SandboxConformanceObservation {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.descendant_limit = 2;
    shape.descendant_required = 3;
    shape.terminal = TerminalShape::Pty;
    let violated = checked_plan(fixture, subject.workspace(), shape, 6).is_err();
    vacuous(
        if violated { SandboxDecision::Violation } else { SandboxDecision::Allowed },
        Vec::new(),
        fixture,
    )
}

fn resource_boundary(
    subject: &LinuxConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let plan = checked_plan(
        fixture,
        subject.workspace(),
        PlanShape::baseline(fixture.resource_limit()),
        7,
    )
    .map_err(|_| ())?;
    let resources = ResourcePlan::from_sandbox(&plan);
    if fixture.resource_requested() > resources.output_bytes() {
        return Ok(vacuous(SandboxDecision::Violation, Vec::new(), fixture));
    }
    subject.run_session(
        &plan,
        fixture,
        SessionOutcome::new(
            SandboxDecision::Allowed,
            false,
            false,
            false,
            fixture.resource_requested(),
        ),
    )
}

fn unsupported(
    subject: &LinuxConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let plan = checked_plan(
        fixture,
        subject.workspace(),
        PlanShape::baseline(fixture.resource_limit()),
        8,
    )
    .map_err(|_| ())?;
    let descriptor = BackendDescriptor::new(
        BackendName::new("unsupported-linux-conformance").map_err(|_| ())?,
        BackendVersion::new("1").map_err(|_| ())?,
        BackendKind::Native,
        PathSemantics::UnixNative,
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
    subject: &LinuxConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let plan = checked_plan(
        fixture,
        subject.workspace(),
        PlanShape::baseline(fixture.resource_limit()),
        9,
    )
    .map_err(|_| ())?;
    subject.run_session(
        &plan,
        fixture,
        SessionOutcome::new(SandboxDecision::Cancelled, true, false, false, 0),
    )
}

fn observation_binding(
    subject: &LinuxConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.filesystem = FileShape::Allow;
    let plan = checked_plan(fixture, subject.workspace(), shape, 10).map_err(|_| ())?;
    subject.run_session(
        &plan,
        fixture,
        SessionOutcome::new(SandboxDecision::Allowed, false, false, false, 0),
    )
}

fn vacuous(
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
