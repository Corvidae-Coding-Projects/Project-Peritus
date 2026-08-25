//! Focused constructors for exact role, child, and directive records.

use peritus_collaboration::{CollaborationId, CollaborationTaskId};
use peritus_review::DispositionKind;
use peritus_scheduler::{DispatchId, SchedulerId, WorkId, WorkerId};
use peritus_types::{
    ActorId, EventId, EventSequence, FindingId, RevisionNumber, RevisionTuple, RunId, Sha256Digest,
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

use super::Scenario;

pub const fn bytes(value: u16) -> [u8; 16] {
    let [high, low] = value.to_be_bytes();
    let mut output = [1; 16];
    output[0] = high;
    output[1] = low;
    output
}

pub const fn digest(value: u16) -> Sha256Digest {
    let [high, low] = value.to_be_bytes();
    let mut output = [1; 32];
    output[0] = high;
    output[1] = low;
    Sha256Digest::new(output)
}

pub fn head(
    aggregate: ChildAggregateKind,
    seed: u16,
    terminal: Option<ChildTerminalClass>,
) -> ChildHead {
    ChildHead::new(
        aggregate,
        EventSequence::new(u64::from(seed)).expect("positive child sequence"),
        EventId::new(bytes(seed)).expect("child event identity"),
        digest(seed),
        terminal,
    )
    .expect("checked child head")
}

pub fn candidate(
    scenario: &Scenario,
    revision: RevisionTuple,
    producer: ActorId,
    ancestry: Sha256Digest,
    seed: u16,
) -> CandidateBinding {
    CandidateBinding::new(
        revision,
        SnapshotId::new(bytes(seed)).expect("snapshot identity"),
        digest(seed + 1),
        digest(seed + 2),
        digest(seed + 3),
        None,
        None,
        vec![producer],
        vec![ancestry],
        scenario.state().limits(),
    )
    .expect("checked candidate")
}

pub fn next_revision(revision: RevisionTuple) -> RevisionTuple {
    RevisionTuple::new(
        revision.acceptance_spec_id(),
        revision.harness_id(),
        revision.workspace_id(),
        revision.workspace_generation(),
        RevisionNumber::new(revision.workspace_revision().get() + 1).expect("successor revision"),
        revision.policy_id(),
        revision.provider_profile_id(),
    )
}

pub fn rebound_cycle(
    state: &crate::OrchestratorState,
    candidate: &CandidateBinding,
    seed: u16,
) -> QualityCycleBinding {
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
    .expect("same-revision writer cycle")
}

pub fn fresh_cycle(candidate: &CandidateBinding, seed: u16) -> QualityCycleBinding {
    QualityCycleBinding::new(
        candidate.revision(),
        RunId::new(bytes(seed)).expect("gate run"),
        RunId::new(bytes(seed + 1)).expect("scheduler run"),
        RunId::new(bytes(seed + 2)).expect("collaboration run"),
        digest(seed + 3),
        digest(seed + 4),
        SchedulerId::new(bytes(seed + 5)).expect("scheduler identity"),
        digest(seed + 6),
        CollaborationId::new(bytes(seed + 7)).expect("collaboration identity"),
        digest(seed + 8),
    )
    .expect("fresh successor cycle")
}

pub fn handoff(
    state: &crate::OrchestratorState,
    kind: HandoffKind,
    findings: Vec<FindingId>,
    seed: u16,
) -> Handoff {
    let ownership = match kind {
        HandoffKind::Writer => None,
        HandoffKind::Reviewer => Some((
            state.ownership().service_actor(),
            state.ownership().reviewers()[0].actor(),
            None,
        )),
        HandoffKind::Fixer => Some((
            state.ownership().reviewers()[0].actor(),
            state.ownership().fixer().actor(),
            Some(TurnId::new(bytes(seed + 1)).expect("fixer turn")),
        )),
    };
    let (source, destination, turn) =
        ownership.expect("genesis already owns the only writer handoff");
    Handoff::new(
        HandoffId::new(bytes(seed)).expect("handoff identity"),
        kind,
        source,
        destination,
        state.current_candidate().clone(),
        turn,
        CollaborationTaskId::new(bytes(seed + 2)).expect("task identity"),
        WorkId::new(bytes(seed + 3)).expect("work identity"),
        vec![digest(seed + 4)],
        vec![digest(seed + 5)],
        findings,
        state.limits(),
    )
    .expect("checked role handoff")
}

pub fn activation(
    state: &crate::OrchestratorState,
    handoff: &Handoff,
    seed: u16,
) -> HandoffActivationObservation {
    HandoffActivationObservation::from_wire(
        handoff.id(),
        handoff.task_id(),
        handoff.work_id(),
        DispatchId::new(bytes(seed)).expect("dispatch identity"),
        WorkerId::new(bytes(seed + 1)).expect("worker identity"),
        handoff.destination_actor(),
        handoff.destination_role().harness_role().expect("executable role"),
        state.current_quality_cycle().scheduler_run_id(),
        state.current_quality_cycle().collaboration_run_id(),
        state.current_candidate().revision(),
        head(ChildAggregateKind::Scheduler, seed + 2, None),
        head(ChildAggregateKind::Collaboration, seed + 3, None),
    )
    .expect("checked D3 activation")
}

pub fn agent_observation(
    state: &crate::OrchestratorState,
    handoff: &Handoff,
    proposal: Option<Sha256Digest>,
    responses: Vec<FixerResponseIdentity>,
    terminal: ChildTerminalClass,
    seed: u16,
) -> AgentChildObservation {
    AgentChildObservation::from_wire(
        handoff.id(),
        handoff.task_id(),
        handoff.work_id(),
        handoff.turn_id().expect("agent handoff has turn"),
        state.binding().run_id(),
        handoff.destination_actor(),
        handoff.destination_role().harness_role().expect("agent role"),
        state.binding().attempt_id(),
        handoff.candidate().revision(),
        proposal,
        responses,
        head(ChildAggregateKind::Agent, seed, Some(terminal)),
    )
    .expect("checked D0 observation")
}

pub fn gate_observation(
    state: &crate::OrchestratorState,
    class: GateObservationClass,
    seed: u16,
) -> GateChildObservation {
    let terminal = match class {
        GateObservationClass::Passed => ChildTerminalClass::Completed,
        GateObservationClass::CandidateFailed | GateObservationClass::InfrastructureFailed => {
            ChildTerminalClass::Failed
        }
        GateObservationClass::Cancelled => ChildTerminalClass::Cancelled,
        GateObservationClass::Indeterminate => ChildTerminalClass::Indeterminate,
    };
    GateChildObservation::from_wire(&GateObservationWire {
        orchestrator_run_id: state.binding().run_id(),
        gate_run_id: state.current_quality_cycle().gate_run_id(),
        revision: state.current_candidate().revision(),
        plan_digest: state.current_quality_cycle().gate_plan_digest(),
        snapshot_digest: state.current_candidate().quality_snapshot_digest(),
        evidence_digest: digest(seed),
        class,
        head: ChildHead::new(
            ChildAggregateKind::Gates,
            EventSequence::new(u64::from(seed)).expect("gate sequence"),
            EventId::new(bytes(seed)).expect("gate event"),
            crate::wire::fixture_tests::digest(70),
            Some(terminal),
        )
        .expect("gate head"),
    })
    .expect("checked D1 observation")
}

pub fn review_observation(
    state: &crate::OrchestratorState,
    class: ReviewObservationClass,
    findings: Vec<FindingId>,
    seed: u16,
) -> ReviewChildObservation {
    let terminal = match class {
        ReviewObservationClass::NeedsFix => None,
        ReviewObservationClass::Completed => Some(ChildTerminalClass::Completed),
        ReviewObservationClass::NeedsHuman => Some(ChildTerminalClass::NeedsHuman),
        ReviewObservationClass::Failed => Some(ChildTerminalClass::Failed),
        ReviewObservationClass::Cancelled => Some(ChildTerminalClass::Cancelled),
    };
    ReviewChildObservation::from_wire(
        state.binding().run_id(),
        state.current_candidate().revision(),
        state.current_quality_cycle().review_binding_digest(),
        true,
        findings,
        class,
        ChildHead::new(
            ChildAggregateKind::Review,
            EventSequence::new(u64::from(seed)).expect("review sequence"),
            EventId::new(bytes(seed)).expect("review event"),
            crate::wire::fixture_tests::digest(71),
            terminal,
        )
        .expect("review head"),
    )
    .expect("checked D2 observation")
}

pub fn infrastructure(
    state: &crate::OrchestratorState,
    seed: u16,
) -> (SchedulerChildObservation, CollaborationChildObservation) {
    let cycle = state.current_quality_cycle();
    (
        SchedulerChildObservation::from_wire(
            cycle.scheduler_run_id(),
            cycle.revision(),
            head(ChildAggregateKind::Scheduler, seed, Some(ChildTerminalClass::Completed)),
        )
        .expect("terminal scheduler"),
        CollaborationChildObservation::from_wire(
            cycle.collaboration_run_id(),
            cycle.revision(),
            head(ChildAggregateKind::Collaboration, seed + 1, Some(ChildTerminalClass::Completed)),
        )
        .expect("terminal collaboration"),
    )
}

pub fn directive(
    scenario: &Scenario,
    id: DirectiveId,
    destination: DirectiveDestination,
    kind: DirectiveKind,
    payload: Sha256Digest,
    task: Option<CollaborationTaskId>,
    work: Option<WorkId>,
) -> PendingDirective {
    PendingDirective::new(
        id,
        destination,
        kind,
        payload,
        4,
        scenario.next_event_id(),
        task,
        work,
        scenario.state().current_candidate().revision(),
    )
    .expect("checked directive")
}

pub fn handoff_payload(handoff: &Handoff, kind: DirectiveKind) -> Sha256Digest {
    directive_payload_digest(
        kind,
        DirectiveDestination::Collaboration,
        DirectivePayloadBinding::Handoff(handoff),
    )
    .expect("handoff payload")
}

pub fn quality_payload(
    state: &crate::OrchestratorState,
    kind: DirectiveKind,
    destination: DirectiveDestination,
) -> Sha256Digest {
    directive_payload_digest(
        kind,
        destination,
        DirectivePayloadBinding::QualityCycle(state.current_quality_cycle()),
    )
    .expect("quality-cycle payload")
}

pub fn fixer_records(
    state: &crate::OrchestratorState,
    handoff: &Handoff,
    response_digest: Sha256Digest,
    seed: u16,
) -> (FixerResponseIdentity, ReviewFixerObservation) {
    let finding = handoff.blocking_findings()[0];
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
        head(ChildAggregateKind::Review, seed + 2, None),
    )
    .expect("D2 fixer response observation");
    (identity, observation)
}
