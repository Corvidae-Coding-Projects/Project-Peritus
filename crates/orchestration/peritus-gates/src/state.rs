//! Complete authoritative D1 run state.

use peritus_quality_policy::GateAttemptOrdinal;
use peritus_types::{
    ActionId, EventId, EventSequence, GateExecutionId, GateId, RevisionTuple, RunId, Sha256Digest,
};

use crate::{GateAttemptResult, GateEvidenceReceipt, GatePlan};

mod checkpoint;
pub mod mutation;

/// One prepared or dispatched attempt and all replay/idempotency bindings.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ActiveAttempt {
    execution_id: GateExecutionId,
    ordinal: GateAttemptOrdinal,
    action_id: ActionId,
    prepared_digest: Sha256Digest,
    replay_digest: Sha256Digest,
    snapshot_digest: Sha256Digest,
}

impl ActiveAttempt {
    /// Creates an exact prepared attempt binding.
    #[must_use]
    pub const fn new(
        execution_id: GateExecutionId,
        ordinal: GateAttemptOrdinal,
        action_id: ActionId,
        prepared_digest: Sha256Digest,
        replay_digest: Sha256Digest,
        snapshot_digest: Sha256Digest,
    ) -> Self {
        Self { execution_id, ordinal, action_id, prepared_digest, replay_digest, snapshot_digest }
    }

    /// Returns the execution identity.
    #[must_use]
    pub const fn execution_id(self) -> GateExecutionId {
        self.execution_id
    }
    /// Returns the one-based attempt ordinal.
    #[must_use]
    pub const fn ordinal(self) -> GateAttemptOrdinal {
        self.ordinal
    }
    /// Returns the fresh authorized action identity.
    #[must_use]
    pub const fn action_id(self) -> ActionId {
        self.action_id
    }
    /// Returns the prepared C4 call digest.
    #[must_use]
    pub const fn prepared_digest(self) -> Sha256Digest {
        self.prepared_digest
    }
    /// Returns the exact C4 replay digest.
    #[must_use]
    pub const fn replay_digest(self) -> Sha256Digest {
        self.replay_digest
    }
    /// Returns the clean C1 snapshot binding digest.
    #[must_use]
    pub const fn snapshot_digest(self) -> Sha256Digest {
        self.snapshot_digest
    }
}

/// Closed lifecycle of one planned gate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateSlotPhase {
    /// Waiting for declared prerequisites.
    Pending,
    /// Prepared durably but not yet dispatched.
    Prepared,
    /// C4 dispatch was durably observed and may own an active effect.
    Dispatched,
    /// A retryable result requires reconciliation first.
    RecoveryPending,
    /// A fresh action may be prepared below the attempt cap.
    RetryPending,
    /// A passing result awaits exact evidence publication.
    EvidencePending,
    /// Passing result and all required evidence are durable and fresh.
    Passed,
    /// This gate reached a non-passing terminal.
    Failed,
    /// A declared prerequisite did not pass.
    Blocked,
    /// Cancellation completed without a pass.
    Cancelled,
}

/// Complete per-gate state in canonical plan order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateSlot {
    gate_id: GateId,
    phase: GateSlotPhase,
    attempts: u16,
    active: Option<ActiveAttempt>,
    last_result: Option<GateAttemptResult>,
    result_event: Option<EventId>,
    evidence: Option<GateEvidenceReceipt>,
    blocked_by: Option<GateId>,
}

impl GateSlot {
    pub(crate) const fn pending(gate_id: GateId) -> Self {
        Self {
            gate_id,
            phase: GateSlotPhase::Pending,
            attempts: 0,
            active: None,
            last_result: None,
            result_event: None,
            evidence: None,
            blocked_by: None,
        }
    }

    /// Returns the planned gate identity.
    #[must_use]
    pub const fn gate_id(&self) -> GateId {
        self.gate_id
    }
    /// Returns the current closed phase.
    #[must_use]
    pub const fn phase(&self) -> GateSlotPhase {
        self.phase
    }
    /// Returns attempts prepared so far.
    #[must_use]
    pub const fn attempts(&self) -> u16 {
        self.attempts
    }
    /// Returns the current attempt binding when one exists.
    #[must_use]
    pub const fn active_attempt(&self) -> Option<ActiveAttempt> {
        self.active
    }
    /// Borrows the latest normalized result.
    #[must_use]
    pub const fn last_result(&self) -> Option<&GateAttemptResult> {
        self.last_result.as_ref()
    }
    /// Returns the exact C0 event carrying the latest normalized result.
    #[must_use]
    pub const fn result_event(&self) -> Option<EventId> {
        self.result_event
    }
    /// Borrows exact published evidence.
    #[must_use]
    pub const fn evidence(&self) -> Option<&GateEvidenceReceipt> {
        self.evidence.as_ref()
    }
    /// Returns the deterministic blocking prerequisite.
    #[must_use]
    pub const fn blocked_by(&self) -> Option<GateId> {
        self.blocked_by
    }
}

/// Nonterminal run phase retained across an explicit pause.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateResumePhase {
    /// Gate scheduling and observation were active.
    Active,
    /// Cancellation was already settling owned effects.
    Cancelling,
}

/// Closed run lifecycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateRunPhase {
    /// Gate scheduling and observation are active.
    Active,
    /// Progress is suspended while the exact prior nonterminal phase is retained.
    Paused(GateResumePhase),
    /// No new dispatch may begin; owned effects must reach terminal observations.
    Cancelling,
    /// Deterministic terminal aggregation was committed.
    Terminal,
}

/// Closed terminal aggregate kind.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateTerminalKind {
    /// Every declared gate passed with fresh complete evidence.
    Passed,
    /// At least one gate authoritatively failed or exhausted attempts.
    Failed,
    /// Cancellation completed and no gate result was implied successful.
    Cancelled,
    /// Recovery could not establish trustworthy effect/evidence state.
    Indeterminate,
}

/// Canonical terminal summary independent from observation arrival order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateTerminal {
    kind: GateTerminalKind,
    non_passing: Vec<GateId>,
    digest: Sha256Digest,
}

impl GateTerminal {
    /// Returns the closed aggregate result.
    #[must_use]
    pub const fn kind(&self) -> GateTerminalKind {
        self.kind
    }
    /// Borrows every non-passing gate in canonical gate order.
    #[must_use]
    pub fn non_passing(&self) -> &[GateId] {
        &self.non_passing
    }
    /// Returns the terminal summary digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Complete replayable authoritative state for one gate run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateRunState {
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
}

impl GateRunState {
    pub(crate) fn genesis(
        plan: &GatePlan,
        snapshot_digest: Sha256Digest,
        sequence: EventSequence,
        event_id: EventId,
    ) -> Self {
        Self {
            run_id: plan.run_id(),
            plan_digest: plan.digest(),
            revision: plan.revision(),
            snapshot_digest,
            maximum_attempts: plan.maximum_attempts(),
            phase: GateRunPhase::Active,
            sequence,
            last_event_id: event_id,
            state_digest: Sha256Digest::new([0; 32]),
            slots: plan.gates().iter().map(|gate| GateSlot::pending(gate.id())).collect(),
            used_executions: Vec::new(),
            used_actions: Vec::new(),
            terminal: None,
        }
    }

    /// Returns the run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns the exact plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }
    /// Returns the exact run revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the clean C1 snapshot digest.
    #[must_use]
    pub const fn snapshot_digest(&self) -> Sha256Digest {
        self.snapshot_digest
    }
    /// Returns the per-gate attempt cap.
    #[must_use]
    pub const fn maximum_attempts(&self) -> u16 {
        self.maximum_attempts
    }
    /// Returns the current run phase.
    #[must_use]
    pub const fn phase(&self) -> GateRunPhase {
        self.phase
    }
    /// Returns the latest aggregate event sequence.
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    /// Returns the latest aggregate event identity.
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.last_event_id
    }
    /// Returns the digest of the complete canonical state.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    /// Borrows per-gate state in canonical gate order.
    #[must_use]
    pub fn slots(&self) -> &[GateSlot] {
        &self.slots
    }
    /// Borrows every prepared execution identity in preparation order.
    #[must_use]
    pub fn used_executions(&self) -> &[GateExecutionId] {
        &self.used_executions
    }
    /// Borrows every consumed action authority identity in preparation order.
    #[must_use]
    pub fn used_actions(&self) -> &[ActionId] {
        &self.used_actions
    }
    /// Looks up one exact gate slot.
    #[must_use]
    pub fn slot(&self, gate_id: GateId) -> Option<&GateSlot> {
        self.slots
            .binary_search_by_key(&gate_id, GateSlot::gate_id)
            .ok()
            .map(|index| &self.slots[index])
    }
    /// Borrows the deterministic terminal summary.
    #[must_use]
    pub const fn terminal(&self) -> Option<&GateTerminal> {
        self.terminal.as_ref()
    }

    pub(crate) fn slot_mut(&mut self, gate_id: GateId) -> Option<&mut GateSlot> {
        self.slots
            .binary_search_by_key(&gate_id, GateSlot::gate_id)
            .ok()
            .map(|index| &mut self.slots[index])
    }
}
