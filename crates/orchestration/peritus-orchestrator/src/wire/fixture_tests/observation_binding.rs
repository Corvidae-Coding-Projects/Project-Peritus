//! Checkpoint decoding rejects stale D1 snapshots and D2 bindings.

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_types::{EventId, EventSequence, Sha256Digest};

use super::{digest, values};
use crate::child::gates::GateObservationWire;
use crate::{
    ChildAggregateKind, ChildHead, ChildObservation, ChildTerminalClass, GateChildObservation,
    GateObservationClass, OrchestratorCounters, OrchestratorState, OrchestratorStateFrame,
    ReviewChildObservation, ReviewObservationClass,
};

#[test]
fn checkpoint_rejects_stale_gate_snapshot_and_review_binding() {
    let (_, _, genesis) = values();
    let cycle = genesis.current_quality_cycle();
    let revision = genesis.current_candidate().revision();
    let gate = GateChildObservation::from_wire(&GateObservationWire {
        orchestrator_run_id: genesis.binding().run_id(),
        gate_run_id: cycle.gate_run_id(),
        revision,
        plan_digest: cycle.gate_plan_digest(),
        snapshot_digest: digest(90),
        evidence_digest: digest(91),
        class: GateObservationClass::Passed,
        head: terminal_head(ChildAggregateKind::Gates, 90),
    })
    .unwrap();
    let review = ReviewChildObservation::from_wire(
        genesis.binding().run_id(),
        revision,
        digest(92),
        true,
        Vec::new(),
        ReviewObservationClass::Completed,
        terminal_head(ChildAggregateKind::Review, 92),
    )
    .unwrap();

    for observation in [ChildObservation::Gates(gate), ChildObservation::Review(review)] {
        let invalid = with_observation(&genesis, observation);
        let bytes =
            encode_message(&OrchestratorStateFrame::from_state(&invalid), CodecLimits::PRODUCTION)
                .unwrap();
        assert!(decode_message::<OrchestratorStateFrame>(&bytes, CodecLimits::PRODUCTION).is_err());
    }
}

fn terminal_head(aggregate: ChildAggregateKind, seed: u8) -> ChildHead {
    ChildHead::new(
        aggregate,
        EventSequence::new(u64::from(seed)).unwrap(),
        EventId::new([seed; 16]).unwrap(),
        digest(seed),
        Some(ChildTerminalClass::Completed),
    )
    .unwrap()
}

fn with_observation(base: &OrchestratorState, observation: ChildObservation) -> OrchestratorState {
    let zero = assemble(base, observation.clone(), digest(0));
    assemble(base, observation, crate::canonical::state_digest(&zero))
}

fn assemble(
    base: &OrchestratorState,
    observation: ChildObservation,
    state_digest: Sha256Digest,
) -> OrchestratorState {
    let counters = base.counters();
    OrchestratorState::from_wire(
        base.binding().clone(),
        base.ownership().clone(),
        base.phase(),
        base.sequence(),
        base.last_event_id(),
        state_digest,
        base.current_candidate().clone(),
        base.candidate_history().to_vec(),
        base.current_quality_cycle().clone(),
        base.quality_cycle_history().to_vec(),
        base.proposed_candidate().cloned(),
        OrchestratorCounters::from_wire(
            counters.revisions(),
            counters.writer_cycles(),
            counters.fixer_cycles(),
            counters.gate_cycles(),
            counters.review_cycles(),
            counters.handoffs(),
            counters.child_directives(),
            1,
            counters.cancellation_reconciliations(),
        ),
        base.handoffs().to_vec(),
        base.open_handoff().cloned(),
        base.activations().to_vec(),
        vec![observation],
        base.active_children().to_vec(),
        base.pending_directive().cloned(),
        base.acceptance_certificate().cloned(),
        base.cancellation_cause(),
        base.used_commands().to_vec(),
        base.terminal().copied(),
        base.pending_terminal().copied(),
        base.paused_reconciliation().cloned(),
        base.paused_children().to_vec(),
    )
}
