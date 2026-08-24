//! A2 cases executed through macOS checked plans, projections, manifests, and sessions.

use peritus_conformance::{
    SandboxConformanceFixture, SandboxConformanceObservation, SandboxDecision, SandboxDomain,
    SandboxLifecyclePhase, SandboxScenario,
};
use peritus_sandbox::{
    AdmissionProfile, FileOperation, SandboxPath, SandboxResourceKind, TerminalMode, admit_backend,
};

use super::{
    adapter::{MacosConformanceSubject, SessionOutcome},
    fixture::{FileShape, NetworkShape, PlanShape, TerminalShape, checked_plan},
};
use crate::{
    MacosDescriptor, MacosHostProbe, ProcessContainment, ProfileCompiler, ProfileDecision,
    ResourceControlPlan, TerminalMapping,
};

pub(super) fn exercise(
    subject: &MacosConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    match fixture.scenario() {
        SandboxScenario::DefaultDeny => default_deny(subject, fixture),
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

fn default_deny(
    subject: &MacosConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let plan = checked_plan(
        fixture,
        subject.workspace(),
        PlanShape::baseline(fixture.resource_limit()),
        11,
    )
    .map_err(|_| ())?;
    let profile =
        ProfileCompiler::compile(&plan, subject.workspace(), &[], None).map_err(|_| ())?;
    let requested = SandboxPath::new(fixture.filesystem_path()).map_err(|_| ())?;
    let containment = ProcessContainment::from_checked_plan(&plan);
    let resources = ResourceControlPlan::from_checked_plan(
        &plan,
        subject.descriptor().probe().evidence().resources.levels(),
    );
    let terminal = TerminalMapping::from_checked_plan(&plan).map_err(|_| ())?;
    let denied = [
        (
            profile.decide(&requested, FileOperation::Read) != ProfileDecision::Allowed,
            SandboxDomain::Filesystem,
        ),
        (
            !plan.contract().process().root_programs().contains(&requested)
                && containment.descendant_limit() == 0,
            SandboxDomain::Process,
        ),
        (plan.contract().environment().literal_names().is_empty(), SandboxDomain::Environment),
        (plan.contract().network().rules().is_empty(), SandboxDomain::Network),
        (plan.contract().secrets().grants().is_empty(), SandboxDomain::Secret),
        (
            resources.control(SandboxResourceKind::Output).ceiling()
                < fixture.resource_limit().saturating_add(1),
            SandboxDomain::Resource,
        ),
        (matches!(terminal, TerminalMapping::Pipes { .. }), SandboxDomain::Terminal),
    ]
    .into_iter()
    .filter_map(|(is_denied, domain)| is_denied.then_some(domain))
    .collect();
    Ok(vacuous(SandboxDecision::Denied, denied, fixture))
}

fn filesystem_denied(
    subject: &MacosConformanceSubject,
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
    subject: &MacosConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.environment_secret = true;
    let plan = checked_plan(fixture, subject.workspace(), shape, 2).map_err(|_| ())?;
    subject.run_session(
        &plan,
        fixture,
        SessionOutcome {
            decision: SandboxDecision::Allowed,
            cancellation: false,
            process_tree_contained: false,
            terminal_controlled: false,
            resource_observed: 0,
        },
    )
}

fn network_allowed(
    subject: &MacosConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.network = NetworkShape::Allow;
    let plan = checked_plan(fixture, subject.workspace(), shape, 3).map_err(|_| ())?;
    subject.run_session(
        &plan,
        fixture,
        SessionOutcome {
            decision: SandboxDecision::Allowed,
            cancellation: false,
            process_tree_contained: false,
            terminal_controlled: false,
            resource_observed: 0,
        },
    )
}

fn network_denied(
    subject: &MacosConformanceSubject,
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
    subject: &MacosConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.descendant_limit = 2;
    shape.descendant_required = 2;
    shape.terminal = TerminalShape::PtyResize;
    let plan = checked_plan(fixture, subject.workspace(), shape, 5).map_err(|_| ())?;
    let containment = ProcessContainment::from_checked_plan(&plan);
    let terminal = TerminalMapping::from_checked_plan(&plan).map_err(|_| ())?;
    let contained = containment.tree_required() && containment.descendant_limit() == 2;
    let controlled = matches!(terminal, TerminalMapping::Pty { resize: true, .. })
        && plan.requirements().terminal().mode() == TerminalMode::Pty;
    subject.run_session(
        &plan,
        fixture,
        SessionOutcome {
            decision: if contained && controlled {
                SandboxDecision::Allowed
            } else {
                SandboxDecision::Violation
            },
            cancellation: false,
            process_tree_contained: contained,
            terminal_controlled: controlled,
            resource_observed: 0,
        },
    )
}

fn process_terminal_exceeded(
    subject: &MacosConformanceSubject,
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
    subject: &MacosConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let plan = checked_plan(
        fixture,
        subject.workspace(),
        PlanShape::baseline(fixture.resource_limit()),
        7,
    )
    .map_err(|_| ())?;
    let resources = ResourceControlPlan::from_checked_plan(
        &plan,
        subject.descriptor().probe().evidence().resources.levels(),
    );
    if fixture.resource_requested() > resources.control(SandboxResourceKind::Output).ceiling() {
        return Ok(vacuous(SandboxDecision::Violation, Vec::new(), fixture));
    }
    subject.run_session(
        &plan,
        fixture,
        SessionOutcome {
            decision: SandboxDecision::Allowed,
            cancellation: false,
            process_tree_contained: false,
            terminal_controlled: false,
            resource_observed: fixture.resource_requested(),
        },
    )
}

fn unsupported(
    subject: &MacosConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let plan = checked_plan(
        fixture,
        subject.workspace(),
        PlanShape::baseline(fixture.resource_limit()),
        8,
    )
    .map_err(|_| ())?;
    let descriptor =
        MacosDescriptor::from_probe(MacosHostProbe::unsupported_current_host()).map_err(|_| ())?;
    let unsupported =
        admit_backend(&plan, descriptor.descriptor(), AdmissionProfile::Conformance).is_err();
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
    subject: &MacosConformanceSubject,
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
        SessionOutcome {
            decision: SandboxDecision::Cancelled,
            cancellation: true,
            process_tree_contained: false,
            terminal_controlled: false,
            resource_observed: 0,
        },
    )
}

fn observation_binding(
    subject: &MacosConformanceSubject,
    fixture: &SandboxConformanceFixture,
) -> Result<SandboxConformanceObservation, ()> {
    let mut shape = PlanShape::baseline(fixture.resource_limit());
    shape.filesystem = FileShape::Allow;
    let plan = checked_plan(fixture, subject.workspace(), shape, 10).map_err(|_| ())?;
    subject.run_session(
        &plan,
        fixture,
        SessionOutcome {
            decision: SandboxDecision::Allowed,
            cancellation: false,
            process_tree_contained: false,
            terminal_controlled: false,
            resource_observed: 0,
        },
    )
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
