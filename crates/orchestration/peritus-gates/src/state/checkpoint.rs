//! Private reconstruction from a fully decoded D1 checkpoint frame.

use peritus_types::{
    ActionId, EventId, EventSequence, GateExecutionId, GateId, RevisionTuple, RunId, Sha256Digest,
};

use super::{
    ActiveAttempt, GateAttemptResult, GateEvidenceReceipt, GateRunPhase, GateRunState, GateSlot,
    GateSlotPhase, GateTerminal, GateTerminalKind,
};

impl GateSlot {
    #[allow(clippy::too_many_arguments, reason = "checkpoint fields remain explicit")]
    pub(crate) const fn from_checkpoint(
        gate_id: GateId,
        phase: GateSlotPhase,
        attempts: u16,
        active: Option<ActiveAttempt>,
        last_result: Option<GateAttemptResult>,
        result_event: Option<EventId>,
        evidence: Option<GateEvidenceReceipt>,
        blocked_by: Option<GateId>,
    ) -> Self {
        Self { gate_id, phase, attempts, active, last_result, result_event, evidence, blocked_by }
    }
}

impl GateTerminal {
    pub(crate) const fn from_checkpoint(
        kind: GateTerminalKind,
        non_passing: Vec<GateId>,
        digest: Sha256Digest,
    ) -> Self {
        Self { kind, non_passing, digest }
    }
}

impl GateRunState {
    #[allow(clippy::too_many_arguments, reason = "checkpoint fields remain explicit")]
    pub(crate) const fn from_checkpoint(
        run_id: RunId,
        plan_digest: Sha256Digest,
        revision: RevisionTuple,
        snapshot_digest: Sha256Digest,
        maximum_attempts: u16,
        phase: GateRunPhase,
        sequence: EventSequence,
        last_event_id: EventId,
        state_digest: Sha256Digest,
        slots: Vec<GateSlot>,
        used_executions: Vec<GateExecutionId>,
        used_actions: Vec<ActionId>,
        terminal: Option<GateTerminal>,
    ) -> Self {
        Self {
            run_id,
            plan_digest,
            revision,
            snapshot_digest,
            maximum_attempts,
            phase,
            sequence,
            last_event_id,
            state_digest,
            slots,
            used_executions,
            used_actions,
            terminal,
        }
    }
}
