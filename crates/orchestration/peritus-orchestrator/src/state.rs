//! Complete authoritative replayable E0 aggregate state.

use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

use crate::{
    AcceptanceCertificate, CandidateBinding, ChildAggregateKind, ChildObservation, Handoff,
    HandoffActivationObservation, OrchestratorBinding, OrchestratorError, OrchestratorLimits,
    OrchestratorPhase, OrchestratorTerminal, PendingDirective, QualityCycleBinding,
    ResumeReconciliation, RoleOwnership,
};

mod counters;
/// Reducer-confined state mutation primitives.
pub mod mutation;
mod validation;

pub use counters::OrchestratorCounters;
pub use validation::revision_successor;

/// Complete deterministic replayable E0 aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestratorState {
    binding: OrchestratorBinding,
    ownership: RoleOwnership,
    phase: OrchestratorPhase,
    sequence: EventSequence,
    last_event_id: EventId,
    state_digest: Sha256Digest,
    current_candidate: CandidateBinding,
    candidate_history: Vec<CandidateBinding>,
    current_quality_cycle: QualityCycleBinding,
    quality_cycle_history: Vec<QualityCycleBinding>,
    proposed_candidate: Option<CandidateBinding>,
    counters: OrchestratorCounters,
    handoffs: Vec<Handoff>,
    open_handoff: Option<Handoff>,
    activations: Vec<HandoffActivationObservation>,
    observations: Vec<ChildObservation>,
    active_children: Vec<ChildAggregateKind>,
    pending_directive: Option<PendingDirective>,
    acceptance_certificate: Option<AcceptanceCertificate>,
    cancellation_cause: Option<Sha256Digest>,
    used_commands: Vec<CommandId>,
    terminal: Option<OrchestratorTerminal>,
    pending_terminal: Option<OrchestratorTerminal>,
    paused_reconciliation: Option<ResumeReconciliation>,
    paused_children: Vec<ChildAggregateKind>,
}

impl OrchestratorState {
    pub(crate) fn genesis(
        binding: OrchestratorBinding,
        ownership: RoleOwnership,
        candidate: CandidateBinding,
        writer_handoff: Handoff,
        sequence: EventSequence,
        event_id: EventId,
        command_id: CommandId,
    ) -> Self {
        let quality_cycle = QualityCycleBinding::genesis(&binding);
        Self {
            binding,
            ownership,
            phase: OrchestratorPhase::Active(crate::ActivePhase::WriterPending),
            sequence,
            last_event_id: event_id,
            state_digest: Sha256Digest::new([0; 32]),
            current_candidate: candidate.clone(),
            candidate_history: vec![candidate],
            current_quality_cycle: quality_cycle.clone(),
            quality_cycle_history: vec![quality_cycle],
            proposed_candidate: None,
            counters: OrchestratorCounters::genesis(),
            handoffs: vec![writer_handoff.clone()],
            open_handoff: Some(writer_handoff),
            activations: Vec::new(),
            observations: Vec::new(),
            active_children: Vec::new(),
            pending_directive: None,
            acceptance_certificate: None,
            cancellation_cause: None,
            used_commands: vec![command_id],
            terminal: None,
            pending_terminal: None,
            paused_reconciliation: None,
            paused_children: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments, reason = "complete checkpoint fields remain explicit")]
    pub(crate) const fn from_wire(
        binding: OrchestratorBinding,
        ownership: RoleOwnership,
        phase: OrchestratorPhase,
        sequence: EventSequence,
        last_event_id: EventId,
        state_digest: Sha256Digest,
        current_candidate: CandidateBinding,
        candidate_history: Vec<CandidateBinding>,
        current_quality_cycle: QualityCycleBinding,
        quality_cycle_history: Vec<QualityCycleBinding>,
        proposed_candidate: Option<CandidateBinding>,
        counters: OrchestratorCounters,
        handoffs: Vec<Handoff>,
        open_handoff: Option<Handoff>,
        activations: Vec<HandoffActivationObservation>,
        observations: Vec<ChildObservation>,
        active_children: Vec<ChildAggregateKind>,
        pending_directive: Option<PendingDirective>,
        acceptance_certificate: Option<AcceptanceCertificate>,
        cancellation_cause: Option<Sha256Digest>,
        used_commands: Vec<CommandId>,
        terminal: Option<OrchestratorTerminal>,
        pending_terminal: Option<OrchestratorTerminal>,
        paused_reconciliation: Option<ResumeReconciliation>,
        paused_children: Vec<ChildAggregateKind>,
    ) -> Self {
        Self {
            binding,
            ownership,
            phase,
            sequence,
            last_event_id,
            state_digest,
            current_candidate,
            candidate_history,
            current_quality_cycle,
            quality_cycle_history,
            proposed_candidate,
            counters,
            handoffs,
            open_handoff,
            activations,
            observations,
            active_children,
            pending_directive,
            acceptance_certificate,
            cancellation_cause,
            used_commands,
            terminal,
            pending_terminal,
            paused_reconciliation,
            paused_children,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), OrchestratorError> {
        validation::validate(self)
    }

    /// Returns immutable run binding.
    #[must_use]
    pub const fn binding(&self) -> &OrchestratorBinding {
        &self.binding
    }
    /// Returns immutable role ownership.
    #[must_use]
    pub const fn ownership(&self) -> &RoleOwnership {
        &self.ownership
    }
    /// Returns current lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> OrchestratorPhase {
        self.phase
    }
    /// Returns latest one-based event sequence.
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    /// Returns latest event identity.
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.last_event_id
    }
    /// Returns canonical complete-state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    /// Returns exact current candidate.
    #[must_use]
    pub const fn current_candidate(&self) -> &CandidateBinding {
        &self.current_candidate
    }
    /// Returns all candidate revisions in order.
    #[must_use]
    pub fn candidate_history(&self) -> &[CandidateBinding] {
        &self.candidate_history
    }
    /// Returns the exact current D1/D2/D3 child-cycle binding.
    #[must_use]
    pub const fn current_quality_cycle(&self) -> &QualityCycleBinding {
        &self.current_quality_cycle
    }
    /// Returns all historical child-cycle bindings in candidate order.
    #[must_use]
    pub fn quality_cycle_history(&self) -> &[QualityCycleBinding] {
        &self.quality_cycle_history
    }
    /// Returns checked proposal awaiting advancement.
    #[must_use]
    pub const fn proposed_candidate(&self) -> Option<&CandidateBinding> {
        self.proposed_candidate.as_ref()
    }
    /// Returns independent counters.
    #[must_use]
    pub const fn counters(&self) -> OrchestratorCounters {
        self.counters
    }
    /// Returns retained immutable handoffs.
    #[must_use]
    pub fn handoffs(&self) -> &[Handoff] {
        &self.handoffs
    }
    /// Returns current open handoff.
    #[must_use]
    pub const fn open_handoff(&self) -> Option<&Handoff> {
        self.open_handoff.as_ref()
    }
    /// Returns retained D3 activation observations.
    #[must_use]
    pub fn activations(&self) -> &[HandoffActivationObservation] {
        &self.activations
    }
    /// Returns retained child observations.
    #[must_use]
    pub fn children(&self) -> &[ChildObservation] {
        &self.observations
    }
    /// Returns canonical active child kinds.
    #[must_use]
    pub fn active_children(&self) -> &[ChildAggregateKind] {
        &self.active_children
    }
    /// Returns the unique pending directive.
    #[must_use]
    pub const fn pending_directive(&self) -> Option<&PendingDirective> {
        self.pending_directive.as_ref()
    }
    /// Returns current B2 certificate.
    #[must_use]
    pub const fn acceptance_certificate(&self) -> Option<&AcceptanceCertificate> {
        self.acceptance_certificate.as_ref()
    }
    /// Returns committed cancellation cause.
    #[must_use]
    pub const fn cancellation_cause(&self) -> Option<Sha256Digest> {
        self.cancellation_cause
    }
    /// Returns consumed command identities in event order.
    #[must_use]
    pub fn used_commands(&self) -> &[CommandId] {
        &self.used_commands
    }
    /// Returns immutable truthful terminal fact.
    #[must_use]
    pub const fn terminal(&self) -> Option<&OrchestratorTerminal> {
        self.terminal.as_ref()
    }
    /// Returns terminal truth awaiting owned-child settlement.
    #[must_use]
    pub const fn pending_terminal(&self) -> Option<&OrchestratorTerminal> {
        self.pending_terminal.as_ref()
    }
    /// Returns the exact paused child-head checkpoint.
    #[must_use]
    pub const fn paused_reconciliation(&self) -> Option<&ResumeReconciliation> {
        self.paused_reconciliation.as_ref()
    }
    /// Returns canonical active children with a committed pause acknowledgement.
    #[must_use]
    pub fn paused_children(&self) -> &[ChildAggregateKind] {
        &self.paused_children
    }
    /// Returns immutable independent limits.
    #[must_use]
    pub const fn limits(&self) -> OrchestratorLimits {
        self.binding.limits()
    }
}
