//! Checked deterministic records used by the lifecycle qualification journey.

use peritus_collaboration::CollaborationTaskId;
use peritus_review::DispositionKind;
use peritus_scheduler::{DispatchId, WorkId, WorkerId};
use peritus_types::{
    ActorId, EventId, EventSequence, FindingId, RevisionNumber, RevisionTuple, Sha256Digest,
    SnapshotId, TurnId,
};

use crate::child::gates::GateObservationWire;
use crate::{
    AgentChildObservation, CandidateBinding, ChildAggregateKind, ChildHead, ChildTerminalClass,
    CollaborationChildObservation, DirectiveDestination, DirectiveId, DirectiveKind,
    DirectivePayloadBinding, FixerResponseIdentity, GateChildObservation, GateObservationClass,
    Handoff, HandoffActivationObservation, HandoffId, HandoffKind, PendingDirective,
    QualityCycleBinding, ReviewChildObservation, ReviewFixerObservation, ReviewFixerRecord,
    ReviewObservationClass, SchedulerChildObservation, directive_payload_digest,
};

use super::scenario::{Scenario, bytes};

pub(super) const fn digest(value: u16) -> Sha256Digest {
    let [high, low] = value.to_be_bytes();
    let mut output = [1; 32];
    output[0] = high;
    output[1] = low;
    Sha256Digest::new(output)
}

fn head(
    aggregate: ChildAggregateKind,
    seed: u16,
    terminal: Option<ChildTerminalClass>,
) -> Result<ChildHead, &'static str> {
    ChildHead::new(
        aggregate,
        EventSequence::new(u64::from(seed)).map_err(|_| "construct child sequence")?,
        EventId::new(bytes(seed)).map_err(|_| "construct child event identity")?,
        digest(seed),
        terminal,
    )
    .map_err(|_| "construct child checkpoint head")
}

pub(super) fn candidate(
    scenario: &Scenario,
    revision: RevisionTuple,
    producer: ActorId,
    ancestry: Sha256Digest,
    seed: u16,
) -> Result<CandidateBinding, &'static str> {
    CandidateBinding::new(
        revision,
        SnapshotId::new(bytes(seed)).map_err(|_| "construct candidate snapshot identity")?,
        digest(seed + 1),
        digest(seed + 2),
        digest(seed + 3),
        None,
        None,
        vec![producer],
        vec![ancestry],
        scenario.state().limits(),
    )
    .map_err(|_| "construct qualification candidate")
}

pub(super) fn next_revision(revision: RevisionTuple) -> Result<RevisionTuple, &'static str> {
    Ok(RevisionTuple::new(
        revision.acceptance_spec_id(),
        revision.harness_id(),
        revision.workspace_id(),
        revision.workspace_generation(),
        RevisionNumber::new(
            revision
                .workspace_revision()
                .get()
                .checked_add(1)
                .ok_or("qualification revision overflowed")?,
        )
        .map_err(|_| "construct successor workspace revision")?,
        revision.policy_id(),
        revision.provider_profile_id(),
    ))
}

pub(super) fn rebound_cycle(
    state: &crate::OrchestratorState,
    candidate: &CandidateBinding,
    seed: u16,
) -> Result<QualityCycleBinding, &'static str> {
    let prior = state.current_quality_cycle();
    QualityCycleBinding::new(
        candidate.revision(),
        prior.gate_run_id(),
        prior.scheduler_run_id(),
        prior.collaboration_run_id(),
        digest(seed),
        digest(seed + 1),
        prior.scheduler_id(),
        prior.scheduler_binding_digest(),
        prior.collaboration_id(),
        prior.collaboration_binding_digest(),
    )
    .map_err(|_| "construct writer-rebound quality cycle")
}

pub(super) fn handoff(
    state: &crate::OrchestratorState,
    kind: HandoffKind,
    findings: Vec<FindingId>,
    seed: u16,
) -> Result<Handoff, &'static str> {
    let (source, destination, turn) = match kind {
        HandoffKind::Writer => return Err("qualification journey cannot replace genesis writer"),
        HandoffKind::Reviewer => {
            (state.ownership().service_actor(), state.ownership().reviewers()[0].actor(), None)
        }
        HandoffKind::Fixer => (
            state.ownership().reviewers()[0].actor(),
            state.ownership().fixer().actor(),
            Some(TurnId::new(bytes(seed + 1)).map_err(|_| "construct fixer turn identity")?),
        ),
    };
    Handoff::new(
        HandoffId::new(bytes(seed)).map_err(|_| "construct handoff identity")?,
        kind,
        source,
        destination,
        state.current_candidate().clone(),
        turn,
        CollaborationTaskId::new(bytes(seed + 2))
            .map_err(|_| "construct collaboration task identity")?,
        WorkId::new(bytes(seed + 3)).map_err(|_| "construct work identity")?,
        vec![digest(seed + 4)],
        vec![digest(seed + 5)],
        findings,
        state.limits(),
    )
    .map_err(|_| "construct qualification handoff")
}

pub(super) fn activation(
    state: &crate::OrchestratorState,
    handoff: &Handoff,
    seed: u16,
) -> Result<HandoffActivationObservation, &'static str> {
    HandoffActivationObservation::from_wire(
        handoff.id(),
        handoff.task_id(),
        handoff.work_id(),
        DispatchId::new(bytes(seed)).map_err(|_| "construct dispatch identity")?,
        WorkerId::new(bytes(seed + 1)).map_err(|_| "construct worker identity")?,
        handoff.destination_actor(),
        handoff
            .destination_role()
            .harness_role()
            .ok_or("handoff destination has no executable role")?,
        state.current_quality_cycle().scheduler_run_id(),
        state.current_quality_cycle().collaboration_run_id(),
        state.current_candidate().revision(),
        head(ChildAggregateKind::Scheduler, seed + 2, None)?,
        head(ChildAggregateKind::Collaboration, seed + 3, None)?,
    )
    .map_err(|_| "construct handoff activation observation")
}

pub(super) fn agent_observation(
    state: &crate::OrchestratorState,
    handoff: &Handoff,
    proposal: Option<Sha256Digest>,
    responses: Vec<FixerResponseIdentity>,
    seed: u16,
) -> Result<AgentChildObservation, &'static str> {
    AgentChildObservation::from_wire(
        handoff.id(),
        handoff.task_id(),
        handoff.work_id(),
        handoff.turn_id().ok_or("agent handoff lacks a turn")?,
        state.binding().run_id(),
        handoff.destination_actor(),
        handoff.destination_role().harness_role().ok_or("agent handoff has no executable role")?,
        state.binding().attempt_id(),
        handoff.candidate().revision(),
        proposal,
        responses,
        head(ChildAggregateKind::Agent, seed, Some(ChildTerminalClass::Completed))?,
    )
    .map_err(|_| "construct agent completion observation")
}

pub(super) fn gate_observation(
    state: &crate::OrchestratorState,
    seed: u16,
) -> Result<GateChildObservation, &'static str> {
    GateChildObservation::from_wire(&GateObservationWire {
        orchestrator_run_id: state.binding().run_id(),
        gate_run_id: state.current_quality_cycle().gate_run_id(),
        revision: state.current_candidate().revision(),
        plan_digest: state.current_quality_cycle().gate_plan_digest(),
        snapshot_digest: state.current_candidate().quality_snapshot_digest(),
        evidence_digest: digest(seed),
        class: GateObservationClass::Passed,
        head: head(ChildAggregateKind::Gates, seed, Some(ChildTerminalClass::Completed))?,
    })
    .map_err(|_| "construct gate observation")
}

pub(super) fn review_observation(
    state: &crate::OrchestratorState,
    class: ReviewObservationClass,
    findings: Vec<FindingId>,
    seed: u16,
) -> Result<ReviewChildObservation, &'static str> {
    let terminal =
        (class != ReviewObservationClass::NeedsFix).then_some(ChildTerminalClass::Completed);
    ReviewChildObservation::from_wire(
        state.binding().run_id(),
        state.current_candidate().revision(),
        state.current_quality_cycle().review_binding_digest(),
        true,
        findings,
        class,
        head(ChildAggregateKind::Review, seed, terminal)?,
    )
    .map_err(|_| "construct review observation")
}

pub(super) fn infrastructure(
    state: &crate::OrchestratorState,
    seed: u16,
) -> Result<(SchedulerChildObservation, CollaborationChildObservation), &'static str> {
    let cycle = state.current_quality_cycle();
    Ok((
        SchedulerChildObservation::from_wire(
            cycle.scheduler_run_id(),
            cycle.revision(),
            head(ChildAggregateKind::Scheduler, seed, Some(ChildTerminalClass::Completed))?,
        )
        .map_err(|_| "construct scheduler completion observation")?,
        CollaborationChildObservation::from_wire(
            cycle.collaboration_run_id(),
            cycle.revision(),
            head(ChildAggregateKind::Collaboration, seed + 1, Some(ChildTerminalClass::Completed))?,
        )
        .map_err(|_| "construct collaboration completion observation")?,
    ))
}

pub(super) fn directive(
    scenario: &Scenario,
    id: DirectiveId,
    destination: DirectiveDestination,
    kind: DirectiveKind,
    payload: Sha256Digest,
    task: Option<CollaborationTaskId>,
    work: Option<WorkId>,
) -> Result<PendingDirective, &'static str> {
    PendingDirective::new(
        id,
        destination,
        kind,
        payload,
        4,
        scenario.next_event_id()?,
        task,
        work,
        scenario.state().current_candidate().revision(),
    )
    .map_err(|_| "construct pending directive")
}

pub(super) fn handoff_payload(
    handoff: &Handoff,
    kind: DirectiveKind,
) -> Result<Sha256Digest, &'static str> {
    directive_payload_digest(
        kind,
        DirectiveDestination::Collaboration,
        DirectivePayloadBinding::Handoff(handoff),
    )
    .map_err(|_| "bind handoff directive payload")
}

pub(super) fn quality_payload(
    state: &crate::OrchestratorState,
    kind: DirectiveKind,
    destination: DirectiveDestination,
) -> Result<Sha256Digest, &'static str> {
    directive_payload_digest(
        kind,
        destination,
        DirectivePayloadBinding::QualityCycle(state.current_quality_cycle()),
    )
    .map_err(|_| "bind quality-cycle directive payload")
}

pub(super) fn fixer_records(
    state: &crate::OrchestratorState,
    handoff: &Handoff,
    response_digest: Sha256Digest,
    seed: u16,
) -> Result<(FixerResponseIdentity, ReviewFixerObservation), &'static str> {
    let finding =
        *handoff.blocking_findings().first().ok_or("fixer handoff has no blocking finding")?;
    let identity = FixerResponseIdentity::from_wire(finding, response_digest);
    let record = ReviewFixerRecord::from_wire(
        finding,
        DispositionKind::Fixed,
        handoff.destination_actor(),
        response_digest,
    );
    let observation = ReviewFixerObservation::from_wire(
        handoff.id(),
        state.binding().run_id(),
        handoff.candidate().revision(),
        state.current_quality_cycle().review_binding_digest(),
        vec![record],
        head(ChildAggregateKind::Review, seed + 2, None)?,
    )
    .map_err(|_| "construct fixer review observation")?;
    Ok((identity, observation))
}
