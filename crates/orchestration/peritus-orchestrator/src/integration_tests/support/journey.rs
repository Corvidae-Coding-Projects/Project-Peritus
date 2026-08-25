//! Reusable complete lifecycle routes built only from reducer commands.

use peritus_collaboration::CollaborationTaskId;
use peritus_scheduler::WorkId;
use peritus_types::{FindingId, Sha256Digest};

use crate::{
    ChildTerminalClass, DirectiveDestination, DirectiveId, DirectiveKind, FixerCompletion, Handoff,
    HandoffKind, KernelAcceptanceObservation, KernelAcceptanceOutcome, OrchestratorCommandKind,
    OrchestratorPhase, OrchestratorTerminalKind, PendingDirective, ReviewObservationClass,
};

use super::{
    Scenario, activation, agent_observation, candidate, digest, directive, fixer_records,
    fresh_cycle, gate_observation, handoff, handoff_payload, infrastructure, next_revision,
    quality_payload, rebound_cycle, review_observation,
};

pub fn publish(
    scenario: &mut Scenario,
    destination: DirectiveDestination,
    kind: DirectiveKind,
    payload: Sha256Digest,
    task: Option<CollaborationTaskId>,
    work: Option<WorkId>,
    forced_id: Option<DirectiveId>,
) -> DirectiveId {
    let id = forced_id.unwrap_or_else(|| {
        DirectiveId::new(*scenario.next_event_id().as_bytes()).expect("directive identity")
    });
    let pending = directive(scenario, id, destination, kind, payload, task, work);
    scenario.apply_ok(OrchestratorCommandKind::PublishDirective { directive: pending });
    id
}

pub fn acknowledge(scenario: &mut Scenario, id: DirectiveId) {
    scenario.apply_ok(OrchestratorCommandKind::AcknowledgeDirective { directive_id: id });
}

pub fn activate_open_handoff(scenario: &mut Scenario, seed: u16) -> Handoff {
    let handoff = scenario.state().open_handoff().expect("open handoff").clone();
    let kind = match handoff.kind() {
        HandoffKind::Writer => DirectiveKind::StartWriter,
        HandoffKind::Reviewer => DirectiveKind::StartReview,
        HandoffKind::Fixer => DirectiveKind::StartFixer,
    };
    let id = publish(
        scenario,
        DirectiveDestination::Collaboration,
        kind,
        handoff_payload(&handoff, kind),
        Some(handoff.task_id()),
        Some(handoff.work_id()),
        None,
    );
    acknowledge(scenario, id);
    let observed = activation(scenario.state(), &handoff, seed);
    scenario.apply_ok(OrchestratorCommandKind::ObserveHandoffActivation { activation: observed });
    handoff
}

pub fn complete_writer(scenario: &mut Scenario, seed: u16) {
    let handoff = scenario.state().open_handoff().expect("writer handoff").clone();
    let proposal = digest(seed);
    let output = candidate(
        scenario,
        scenario.state().current_candidate().revision(),
        scenario.state().ownership().writer().actor(),
        proposal,
        seed + 1,
    );
    let cycle = rebound_cycle(scenario.state(), &output, seed + 10);
    let observation = agent_observation(
        scenario.state(),
        &handoff,
        Some(proposal),
        Vec::new(),
        ChildTerminalClass::Completed,
        seed + 12,
    );
    scenario.apply_ok(OrchestratorCommandKind::ObserveWriter {
        observation,
        candidate: Some(output),
        quality_cycle: Some(cycle),
    });
}

pub fn start_gates(scenario: &mut Scenario) {
    let payload =
        quality_payload(scenario.state(), DirectiveKind::StartGates, DirectiveDestination::Gates);
    let id = publish(
        scenario,
        DirectiveDestination::Gates,
        DirectiveKind::StartGates,
        payload,
        None,
        None,
        None,
    );
    acknowledge(scenario, id);
}

pub fn pass_gates(scenario: &mut Scenario, seed: u16) -> Handoff {
    let review = handoff(scenario.state(), HandoffKind::Reviewer, Vec::new(), seed);
    let gates = gate_observation(scenario.state(), crate::GateObservationClass::Passed, seed + 10);
    scenario.apply_ok(OrchestratorCommandKind::ObserveGates {
        observation: gates,
        review_handoff: Some(review.clone()),
    });
    review
}

pub fn complete_review(scenario: &mut Scenario, seed: u16) {
    let observation =
        review_observation(scenario.state(), ReviewObservationClass::Completed, Vec::new(), seed);
    scenario.apply_ok(OrchestratorCommandKind::ObserveReview { observation, fixer_handoff: None });
}

pub fn quiesce_role_infrastructure(scenario: &mut Scenario, seed: u16) {
    let payload = quality_payload(
        scenario.state(),
        DirectiveKind::FinalizeChildren,
        DirectiveDestination::Collaboration,
    );
    let id = publish(
        scenario,
        DirectiveDestination::Collaboration,
        DirectiveKind::FinalizeChildren,
        payload,
        None,
        None,
        None,
    );
    acknowledge(scenario, id);
    let (scheduler, collaboration) = infrastructure(scenario.state(), seed);
    scenario
        .apply_ok(OrchestratorCommandKind::ObserveRoleInfrastructure { scheduler, collaboration });
}

pub fn record_certificate(scenario: &mut Scenario) -> crate::AcceptanceCertificate {
    let certificate = crate::wire::fixture_tests::certificate(scenario.state());
    let id = publish(
        scenario,
        DirectiveDestination::QualityEvaluator,
        DirectiveKind::EvaluateAcceptance,
        certificate.evaluation_request_digest(),
        None,
        None,
        None,
    );
    acknowledge(scenario, id);
    scenario.apply_ok(OrchestratorCommandKind::RecordAcceptanceCertificate {
        certificate: certificate.clone(),
    });
    certificate
}

pub fn accept_in_kernel(
    scenario: &mut Scenario,
    certificate: &crate::AcceptanceCertificate,
    seed: u16,
) {
    let plan = certificate.kernel_plan();
    let begin_id = DirectiveId::new(*plan.begin_event_id().as_bytes()).expect("begin directive");
    publish(
        scenario,
        DirectiveDestination::Kernel,
        DirectiveKind::BeginKernelAcceptance,
        certificate.begin_payload_digest(),
        None,
        None,
        Some(begin_id),
    );
    acknowledge(scenario, begin_id);
    scenario.apply_ok(OrchestratorCommandKind::ObserveKernelAcceptance {
        observation: KernelAcceptanceObservation::from_wire(
            plan.begin_event_id(),
            plan.begin_command_id(),
            peritus_types::EventSequence::new(u64::from(seed)).expect("kernel sequence"),
            plan.expected_previous_kernel_event(),
            scenario.state().binding().run_id(),
            scenario.state().current_candidate().revision(),
            KernelAcceptanceOutcome::Begun,
        ),
    });
    assert_eq!(scenario.state().terminal(), None);

    let evaluate_id =
        DirectiveId::new(*plan.evaluate_event_id().as_bytes()).expect("evaluate directive");
    publish(
        scenario,
        DirectiveDestination::Kernel,
        DirectiveKind::EvaluateKernelAcceptance,
        certificate.evaluate_payload_digest(),
        None,
        None,
        Some(evaluate_id),
    );
    acknowledge(scenario, evaluate_id);
    scenario.apply_ok(OrchestratorCommandKind::ObserveKernelAcceptance {
        observation: KernelAcceptanceObservation::from_wire(
            plan.evaluate_event_id(),
            plan.evaluate_command_id(),
            peritus_types::EventSequence::new(u64::from(seed + 1)).expect("kernel sequence"),
            Some(plan.evaluate_previous_event_id()),
            scenario.state().binding().run_id(),
            scenario.state().current_candidate().revision(),
            KernelAcceptanceOutcome::Accepted,
        ),
    });
}

pub fn happy_path() -> Scenario {
    let mut scenario = Scenario::new();
    activate_open_handoff(&mut scenario, 300);
    complete_writer(&mut scenario, 310);
    start_gates(&mut scenario);
    pass_gates(&mut scenario, 330);
    activate_open_handoff(&mut scenario, 350);
    complete_review(&mut scenario, 360);
    quiesce_role_infrastructure(&mut scenario, 370);
    let certificate = record_certificate(&mut scenario);
    accept_in_kernel(&mut scenario, &certificate, 380);
    assert_eq!(scenario.state().phase(), OrchestratorPhase::Terminal);
    assert_eq!(
        scenario.state().terminal().map(|terminal| terminal.kind()),
        Some(OrchestratorTerminalKind::Accepted)
    );
    scenario
}

pub fn fix_cycle_path() -> Scenario {
    let mut scenario = Scenario::new();
    activate_open_handoff(&mut scenario, 400);
    complete_writer(&mut scenario, 410);
    start_gates(&mut scenario);
    pass_gates(&mut scenario, 430);
    activate_open_handoff(&mut scenario, 450);

    let finding = FindingId::new(super::bytes(460)).expect("finding identity");
    let fixer = handoff(scenario.state(), HandoffKind::Fixer, vec![finding], 461);
    let needs_fix =
        review_observation(scenario.state(), ReviewObservationClass::NeedsFix, vec![finding], 470);
    scenario.apply_ok(OrchestratorCommandKind::ObserveReview {
        observation: needs_fix,
        fixer_handoff: Some(fixer.clone()),
    });
    activate_open_handoff(&mut scenario, 480);

    let response_digest = digest(490);
    let (response, review_response) = fixer_records(scenario.state(), &fixer, response_digest, 491);
    let proposal_digest = digest(495);
    let proposal = candidate(
        &scenario,
        next_revision(scenario.state().current_candidate().revision()),
        scenario.state().ownership().fixer().actor(),
        proposal_digest,
        496,
    );
    let observation = agent_observation(
        scenario.state(),
        &fixer,
        Some(proposal_digest),
        vec![response],
        ChildTerminalClass::Completed,
        505,
    );
    let completion = FixerCompletion::new(observation, proposal.clone(), review_response, &fixer)
        .expect("complete fixer response conservation");
    scenario.apply_ok(OrchestratorCommandKind::ObserveFixer { completion });
    quiesce_role_infrastructure(&mut scenario, 510);
    let cycle = fresh_cycle(&proposal, 520);
    scenario.apply_ok(OrchestratorCommandKind::AdvanceCandidate { quality_cycle: cycle });

    start_gates(&mut scenario);
    pass_gates(&mut scenario, 540);
    activate_open_handoff(&mut scenario, 560);
    complete_review(&mut scenario, 570);
    quiesce_role_infrastructure(&mut scenario, 580);
    let certificate = record_certificate(&mut scenario);
    accept_in_kernel(&mut scenario, &certificate, 590);
    scenario
}

pub fn event_bound_directive_id(scenario: &Scenario) -> DirectiveId {
    DirectiveId::new(*scenario.next_event_id().as_bytes()).expect("event-bound directive identity")
}

pub fn pending_for_open_handoff(scenario: &Scenario) -> PendingDirective {
    let handoff = scenario.state().open_handoff().expect("open handoff");
    let kind = match handoff.kind() {
        HandoffKind::Writer => DirectiveKind::StartWriter,
        HandoffKind::Reviewer => DirectiveKind::StartReview,
        HandoffKind::Fixer => DirectiveKind::StartFixer,
    };
    directive(
        scenario,
        event_bound_directive_id(scenario),
        DirectiveDestination::Collaboration,
        kind,
        handoff_payload(handoff, kind),
        Some(handoff.task_id()),
        Some(handoff.work_id()),
    )
}
