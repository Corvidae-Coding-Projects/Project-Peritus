//! Complete lifecycle aggregate and genesis transition.

mod lookup;
mod validation;

use crate::{
    CommandEnvelope, KernelError, KernelErrorKind, KernelEvent, KernelEventKind, KernelSubject,
    SessionState,
};
use peritus_spec::{AcceptanceContract, ContractBinding};
use peritus_types::{CommandId, EventId, EventSequence, ProjectId, RevisionTuple, SessionId};

use crate::{ActionState, AttemptState, ReviewState, RunState, TurnState, WaiverState};
#[doc = " Complete authoritative B0 state for one session event stream."]
#[derive(Debug, Eq, PartialEq)]
pub struct KernelAggregate {
    pub(crate) project_id: ProjectId,
    pub(crate) revision: RevisionTuple,
    pub(crate) contract_binding: ContractBinding,
    pub(crate) session: SessionState,
    pub(crate) head_event_id: EventId,
    pub(crate) last_sequence: EventSequence,
    pub(crate) accepted_command_ids: Vec<CommandId>,
    pub(crate) event_ids: Vec<EventId>,
    pub(crate) runs: Vec<RunState>,
    pub(crate) attempts: Vec<AttemptState>,
    pub(crate) turns: Vec<TurnState>,
    pub(crate) actions: Vec<ActionState>,
    pub(crate) reviews: Vec<ReviewState>,
    pub(crate) waivers: Vec<WaiverState>,
}
#[doc = " Checked first aggregate state and its sequence-one event."]
#[derive(Debug, Eq, PartialEq)]
pub struct KernelGenesis {
    aggregate: KernelAggregate,
    event: KernelEvent,
}
impl KernelGenesis {
    #[doc = " Borrows the initialized aggregate."]
    #[must_use]
    pub const fn aggregate(&self) -> &KernelAggregate {
        &self.aggregate
    }
    #[doc = " Returns the sequence-one session-open event."]
    #[must_use]
    pub const fn event(&self) -> KernelEvent {
        self.event
    }
    #[doc = " Consumes genesis into its aggregate and event."]
    #[must_use]
    pub fn into_parts(self) -> (KernelAggregate, KernelEvent) {
        (self.aggregate, self.event)
    }
}
impl KernelAggregate {
    #[doc = " Opens a new session and emits its sequence-one causal event."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects a stale revision, non-genesis predecessor, or mismatched contract binding."]
    pub fn open(
        project_id: ProjectId,
        session_id: SessionId,
        contract: &AcceptanceContract,
        revision: RevisionTuple,
        envelope: CommandEnvelope,
    ) -> Result<KernelGenesis, KernelError> {
        if !crate::identity::revision_equal(envelope.revision, revision) {
            return Err(KernelError::new(KernelErrorKind::RevisionMismatch));
        }
        if envelope.expected_previous_event_id.is_some() {
            return Err(KernelError::new(KernelErrorKind::CausalHeadMismatch));
        }
        let Ok(binding) = contract.bind(revision) else {
            return Err(KernelError::new(KernelErrorKind::ContractMismatch));
        };
        let sequence = EventSequence::first();
        let event = KernelEvent::new(
            envelope.event_id,
            envelope.command_id,
            sequence,
            None,
            revision,
            KernelEventKind::SessionOpened,
            KernelSubject::Session(session_id),
        );
        let aggregate = Self {
            project_id,
            revision,
            contract_binding: binding,
            session: SessionState::open(session_id),
            head_event_id: envelope.event_id,
            last_sequence: sequence,
            accepted_command_ids: vec![envelope.command_id],
            event_ids: vec![envelope.event_id],
            runs: Vec::new(),
            attempts: Vec::new(),
            turns: Vec::new(),
            actions: Vec::new(),
            reviews: Vec::new(),
            waivers: Vec::new(),
        };
        if !aggregate.is_valid() {
            return Err(KernelError::new(KernelErrorKind::InvalidAggregate));
        }
        Ok(KernelGenesis { aggregate, event })
    }
    #[doc = " Returns the configured project identity."]
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }
    #[doc = " Returns the exact current revision tuple."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the immutable acceptance-contract binding."]
    #[must_use]
    pub const fn contract_binding(&self) -> ContractBinding {
        self.contract_binding
    }
    #[doc = " Returns the session state."]
    #[must_use]
    pub const fn session(&self) -> SessionState {
        self.session
    }
    #[doc = " Returns the current causal head."]
    #[must_use]
    pub const fn head_event_id(&self) -> EventId {
        self.head_event_id
    }
    #[doc = " Returns the latest event sequence."]
    #[must_use]
    pub const fn last_sequence(&self) -> EventSequence {
        self.last_sequence
    }
    #[doc = " Returns all runs in creation order."]
    #[must_use]
    pub const fn runs(&self) -> &[RunState] {
        self.runs.as_slice()
    }
    #[doc = " Returns all attempts in creation order."]
    #[must_use]
    pub const fn attempts(&self) -> &[AttemptState] {
        self.attempts.as_slice()
    }
    #[doc = " Returns all turns in creation order."]
    #[must_use]
    pub const fn turns(&self) -> &[TurnState] {
        self.turns.as_slice()
    }
    #[doc = " Returns all actions in creation order."]
    #[must_use]
    pub const fn actions(&self) -> &[ActionState] {
        self.actions.as_slice()
    }
    #[doc = " Returns all reviews in creation order."]
    #[must_use]
    pub const fn reviews(&self) -> &[ReviewState] {
        self.reviews.as_slice()
    }
    #[doc = " Returns all waivers in creation order."]
    #[must_use]
    pub const fn waivers(&self) -> &[WaiverState] {
        self.waivers.as_slice()
    }
    #[doc = " Returns whether the complete executable aggregate invariants hold."]
    #[must_use]
    pub fn is_valid(&self) -> bool {
        validation::is_valid(self)
    }
}
impl Clone for KernelAggregate {
    fn clone(&self) -> Self {
        let accepted_command_ids = self.accepted_command_ids.clone();
        let event_ids = self.event_ids.clone();
        let runs = self.runs.clone();
        let attempts = self.attempts.clone();
        let turns = self.turns.clone();
        let actions = ActionState::clone_sequence(self.actions.as_slice());
        let reviews = self.reviews.clone();
        let waivers = self.waivers.clone();
        Self {
            project_id: self.project_id,
            revision: self.revision,
            contract_binding: self.contract_binding,
            session: self.session,
            head_event_id: self.head_event_id,
            last_sequence: self.last_sequence,
            accepted_command_ids,
            event_ids,
            runs,
            attempts,
            turns,
            actions,
            reviews,
            waivers,
        }
    }
}
