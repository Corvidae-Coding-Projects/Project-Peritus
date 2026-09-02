//! Shortest legal reducer journeys to every H1 daemon lifecycle phase.

use peritus_types::FindingId;

use crate::{
    DirectiveDestination, DirectiveId, DirectiveKind, FixerCompletion, Handoff, HandoffKind,
    OrchestratorCommandKind, ReviewObservationClass,
};

use super::LifecyclePhase;
use super::certificate;
use super::domain::{
    activation, agent_observation, candidate, digest, directive, fixer_records, gate_observation,
    handoff, handoff_payload, infrastructure, next_revision, quality_payload, rebound_cycle,
    review_observation,
};
use super::scenario::{Scenario, bytes};

pub(super) fn to_phase(target: LifecyclePhase) -> Result<Scenario, &'static str> {
    let mut scenario = Scenario::new()?;
    if target == LifecyclePhase::WriterPending {
        return Ok(scenario);
    }
    activate_open_handoff(&mut scenario, 300)?;
    if target == LifecyclePhase::WriterActive {
        return Ok(scenario);
    }
    complete_writer(&mut scenario, 310)?;
    if target == LifecyclePhase::GatesPending {
        return Ok(scenario);
    }
    start_gates(&mut scenario)?;
    if target == LifecyclePhase::GatesActive {
        return Ok(scenario);
    }
    pass_gates(&mut scenario, 330)?;
    if target == LifecyclePhase::ReviewPending {
        return Ok(scenario);
    }
    activate_open_handoff(&mut scenario, 350)?;
    if target == LifecyclePhase::ReviewActive {
        return Ok(scenario);
    }
    match target {
        LifecyclePhase::FixerPending
        | LifecyclePhase::FixerActive
        | LifecyclePhase::RevisionAdvancing => fixer_path(scenario, target),
        LifecyclePhase::EvaluatingAcceptance | LifecyclePhase::KernelAcceptancePending => {
            acceptance_path(scenario, target)
        }
        _ => Err("lifecycle target was passed without a matching journey"),
    }
}

fn fixer_path(mut scenario: Scenario, target: LifecyclePhase) -> Result<Scenario, &'static str> {
    let finding = FindingId::new(bytes(460)).map_err(|_| "construct review finding identity")?;
    let fixer = handoff(scenario.state(), HandoffKind::Fixer, vec![finding], 461)?;
    let review =
        review_observation(scenario.state(), ReviewObservationClass::NeedsFix, vec![finding], 470)?;
    scenario.apply(OrchestratorCommandKind::ObserveReview {
        observation: review,
        fixer_handoff: Some(fixer.clone()),
    })?;
    if target == LifecyclePhase::FixerPending {
        return Ok(scenario);
    }
    activate_open_handoff(&mut scenario, 480)?;
    if target == LifecyclePhase::FixerActive {
        return Ok(scenario);
    }
    let response_digest = digest(490);
    let (response, review_response) =
        fixer_records(scenario.state(), &fixer, response_digest, 491)?;
    let proposal_digest = digest(495);
    let proposal = candidate(
        &scenario,
        next_revision(scenario.state().current_candidate().revision())?,
        scenario.state().ownership().fixer().actor(),
        proposal_digest,
        496,
    )?;
    let observation =
        agent_observation(scenario.state(), &fixer, Some(proposal_digest), vec![response], 505)?;
    let completion = FixerCompletion::new(observation, proposal, review_response, &fixer)
        .map_err(|_| "construct checked fixer completion")?;
    scenario.apply(OrchestratorCommandKind::ObserveFixer { completion })?;
    Ok(scenario)
}

fn acceptance_path(
    mut scenario: Scenario,
    target: LifecyclePhase,
) -> Result<Scenario, &'static str> {
    let review =
        review_observation(scenario.state(), ReviewObservationClass::Completed, Vec::new(), 360)?;
    scenario.apply(OrchestratorCommandKind::ObserveReview {
        observation: review,
        fixer_handoff: None,
    })?;
    if target == LifecyclePhase::EvaluatingAcceptance {
        return Ok(scenario);
    }
    quiesce_role_infrastructure(&mut scenario, 370)?;
    let certificate = certificate::build(scenario.state())?;
    let directive_id = publish(
        &mut scenario,
        DirectiveDestination::QualityEvaluator,
        DirectiveKind::EvaluateAcceptance,
        certificate.evaluation_request_digest(),
        None,
        None,
    )?;
    acknowledge(&mut scenario, directive_id)?;
    scenario.apply(OrchestratorCommandKind::RecordAcceptanceCertificate { certificate })?;
    Ok(scenario)
}

fn activate_open_handoff(scenario: &mut Scenario, seed: u16) -> Result<Handoff, &'static str> {
    let role_handoff =
        scenario.state().open_handoff().ok_or("lifecycle phase has no open handoff")?.clone();
    let kind = match role_handoff.kind() {
        HandoffKind::Writer => DirectiveKind::StartWriter,
        HandoffKind::Reviewer => DirectiveKind::StartReview,
        HandoffKind::Fixer => DirectiveKind::StartFixer,
    };
    let directive_id = publish(
        scenario,
        DirectiveDestination::Collaboration,
        kind,
        handoff_payload(&role_handoff, kind)?,
        Some(role_handoff.task_id()),
        Some(role_handoff.work_id()),
    )?;
    acknowledge(scenario, directive_id)?;
    let observed = activation(scenario.state(), &role_handoff, seed)?;
    scenario.apply(OrchestratorCommandKind::ObserveHandoffActivation { activation: observed })?;
    Ok(role_handoff)
}

fn complete_writer(scenario: &mut Scenario, seed: u16) -> Result<(), &'static str> {
    let writer = scenario.state().open_handoff().ok_or("writer phase has no handoff")?.clone();
    let proposal_digest = digest(seed);
    let output = candidate(
        scenario,
        scenario.state().current_candidate().revision(),
        scenario.state().ownership().writer().actor(),
        proposal_digest,
        seed + 1,
    )?;
    let cycle = rebound_cycle(scenario.state(), &output, seed + 10)?;
    let observation =
        agent_observation(scenario.state(), &writer, Some(proposal_digest), Vec::new(), seed + 12)?;
    scenario.apply(OrchestratorCommandKind::ObserveWriter {
        observation,
        candidate: Some(output),
        quality_cycle: Some(cycle),
    })
}

fn start_gates(scenario: &mut Scenario) -> Result<(), &'static str> {
    let payload =
        quality_payload(scenario.state(), DirectiveKind::StartGates, DirectiveDestination::Gates)?;
    let directive_id = publish(
        scenario,
        DirectiveDestination::Gates,
        DirectiveKind::StartGates,
        payload,
        None,
        None,
    )?;
    acknowledge(scenario, directive_id)
}

fn pass_gates(scenario: &mut Scenario, seed: u16) -> Result<(), &'static str> {
    let review_handoff = handoff(scenario.state(), HandoffKind::Reviewer, Vec::new(), seed)?;
    let gates = gate_observation(scenario.state(), seed + 10)?;
    scenario.apply(OrchestratorCommandKind::ObserveGates {
        observation: gates,
        review_handoff: Some(review_handoff),
    })
}

fn quiesce_role_infrastructure(scenario: &mut Scenario, seed: u16) -> Result<(), &'static str> {
    let payload = quality_payload(
        scenario.state(),
        DirectiveKind::FinalizeChildren,
        DirectiveDestination::Collaboration,
    )?;
    let directive_id = publish(
        scenario,
        DirectiveDestination::Collaboration,
        DirectiveKind::FinalizeChildren,
        payload,
        None,
        None,
    )?;
    acknowledge(scenario, directive_id)?;
    let (scheduler, collaboration) = infrastructure(scenario.state(), seed)?;
    scenario.apply(OrchestratorCommandKind::ObserveRoleInfrastructure { scheduler, collaboration })
}

fn publish(
    scenario: &mut Scenario,
    destination: DirectiveDestination,
    kind: DirectiveKind,
    payload: peritus_types::Sha256Digest,
    task: Option<peritus_collaboration::CollaborationTaskId>,
    work: Option<peritus_scheduler::WorkId>,
) -> Result<DirectiveId, &'static str> {
    let id = DirectiveId::new(*scenario.next_event_id()?.as_bytes())
        .map_err(|_| "construct directive identity")?;
    let pending = directive(scenario, id, destination, kind, payload, task, work)?;
    scenario.apply(OrchestratorCommandKind::PublishDirective { directive: pending })?;
    Ok(id)
}

fn acknowledge(scenario: &mut Scenario, directive_id: DirectiveId) -> Result<(), &'static str> {
    scenario.apply(OrchestratorCommandKind::AcknowledgeDirective { directive_id })
}
