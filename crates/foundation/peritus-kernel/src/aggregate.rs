//! Complete lifecycle aggregate and genesis transition.

mod lookup;
mod validation;

use crate::{
    CommandEnvelope, KernelError, KernelErrorKind, KernelEvent, KernelEventKind, KernelSubject,
    SessionState,
};
use peritus_spec::{AcceptanceContract, ContractBinding};
use peritus_types::{
    CommandId, EventId, EventSequence, ProjectId, RevisionTuple, SessionId,
};
use vstd::prelude::*;

verus! {

use crate::{ActionState, AttemptState, ReviewState, RunState, TurnState, WaiverState};

/// Complete authoritative B0 state for one session event stream.
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

/// Checked first aggregate state and its sequence-one event.
#[derive(Debug, Eq, PartialEq)]
pub struct KernelGenesis {
    aggregate: KernelAggregate,
    event: KernelEvent,
}

impl KernelGenesis {
    pub(crate) closed spec fn spec_aggregate(&self) -> KernelAggregate { self.aggregate }
    pub(crate) closed spec fn spec_event(&self) -> KernelEvent { self.event }

    /// Borrows the initialized aggregate.
    #[must_use]
    pub const fn aggregate(&self) -> &KernelAggregate { &self.aggregate }
    /// Returns the sequence-one session-open event.
    #[must_use]
    pub const fn event(&self) -> KernelEvent { self.event }
    /// Consumes genesis into its aggregate and event.
    #[must_use]
    pub fn into_parts(self) -> (KernelAggregate, KernelEvent) { (self.aggregate, self.event) }
}

impl KernelAggregate {
    /// Exact scalar fields preserved by cloning.
    pub closed spec fn scalar_clone_equivalent(left: &Self, right: &Self) -> bool {
        left.project_id == right.project_id
            && left.revision == right.revision
            && left.contract_binding == right.contract_binding
            && left.session == right.session
            && left.head_event_id == right.head_event_id
            && left.last_sequence == right.last_sequence
    }

    /// Exact sequence fields preserved by cloning.
    pub closed spec fn sequence_clone_equivalent(left: &Self, right: &Self) -> bool {
        left.accepted_command_ids@ == right.accepted_command_ids@
            && left.event_ids@ == right.event_ids@
            && left.runs@ == right.runs@
            && left.attempts@ == right.attempts@
            && left.turns@ == right.turns@
            && left.reviews@ == right.reviews@
            && left.waivers@ == right.waivers@
    }

    /// Semantic action fields preserved by cloning.
    pub closed spec fn action_clone_equivalent(left: &Self, right: &Self) -> bool {
        ActionState::sequence_clone_equivalent(left.actions@, right.actions@)
    }

    /// Exact scalar/sequence and semantic action fields preserved by cloning.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        Self::scalar_clone_equivalent(left, right)
            && Self::sequence_clone_equivalent(left, right)
            && Self::action_clone_equivalent(left, right)
    }

    /// Specification view of the exact current revision.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }
    /// Specification view of stored review-cycle states.
    pub closed spec fn spec_reviews(&self) -> Seq<ReviewState> { self.reviews@ }
    /// Specification view of stored finding-waiver states.
    pub closed spec fn spec_waivers(&self) -> Seq<WaiverState> { self.waivers@ }

    pub(crate) proof fn expose_internal_views(&self)
        ensures
            self.spec_revision() == self.revision,
            self.spec_reviews() == self.reviews@,
            self.spec_waivers() == self.waivers@,
    {}

    /// Formal shape guaranteed by every successful genesis transition.
    pub closed spec fn genesis_result_refines(
        revision: RevisionTuple,
        envelope: CommandEnvelope,
        result: Result<KernelGenesis, KernelError>,
    ) -> bool {
        match result {
            Ok(genesis) => {
                let aggregate = genesis.aggregate;
                let event = genesis.event;
                &&& crate::identity::revisions_equal(aggregate.revision, revision)
                &&& aggregate.session.phase == crate::SessionPhase::Open
                &&& aggregate.last_sequence.spec_value() == 1
                &&& event.sequence.spec_value() == 1
                &&& event.kind == KernelEventKind::SessionOpened
                &&& event.previous_event_id.is_none()
                &&& crate::identity::event_ids_equal(event.id, envelope.event_id)
                &&& crate::identity::event_ids_equal(aggregate.head_event_id, event.id)
                &&& aggregate.runs@.len() == 0
                &&& aggregate.attempts@.len() == 0
                &&& aggregate.turns@.len() == 0
                &&& aggregate.actions@.len() == 0
                &&& aggregate.reviews@.len() == 0
                &&& aggregate.waivers@.len() == 0
            }
            Err(_) => true,
        }
    }

    /// Opens a new session and emits its sequence-one causal event.
    ///
    /// # Errors
    ///
    /// Rejects a stale revision, non-genesis predecessor, or mismatched contract binding.
    pub fn open(
        project_id: ProjectId,
        session_id: SessionId,
        contract: &AcceptanceContract,
        revision: RevisionTuple,
        envelope: CommandEnvelope,
    ) -> (result: Result<KernelGenesis, KernelError>)
        ensures Self::genesis_result_refines(revision, envelope, result),
    {
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

    /// Returns the configured project identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId { self.project_id }
    /// Returns the exact current revision tuple.
    #[must_use]
    pub const fn revision(&self) -> (revision: RevisionTuple)
        ensures revision == self.spec_revision(),
    { self.revision }
    /// Returns the immutable acceptance-contract binding.
    #[must_use]
    pub const fn contract_binding(&self) -> ContractBinding { self.contract_binding }
    /// Returns the session state.
    #[must_use]
    pub const fn session(&self) -> SessionState { self.session }
    /// Returns the current causal head.
    #[must_use]
    pub const fn head_event_id(&self) -> EventId { self.head_event_id }
    /// Returns the latest event sequence.
    #[must_use]
    pub const fn last_sequence(&self) -> EventSequence { self.last_sequence }
    /// Returns all runs in creation order.
    #[must_use]
    pub const fn runs(&self) -> &[RunState] { self.runs.as_slice() }
    /// Returns all attempts in creation order.
    #[must_use]
    pub const fn attempts(&self) -> &[AttemptState] { self.attempts.as_slice() }
    /// Returns all turns in creation order.
    #[must_use]
    pub const fn turns(&self) -> &[TurnState] { self.turns.as_slice() }
    /// Returns all actions in creation order.
    #[must_use]
    pub const fn actions(&self) -> &[ActionState] { self.actions.as_slice() }
    /// Returns all reviews in creation order.
    #[must_use]
    pub const fn reviews(&self) -> (reviews: &[ReviewState])
        ensures reviews@ == self.spec_reviews(),
    { self.reviews.as_slice() }
    /// Returns all waivers in creation order.
    #[must_use]
    pub const fn waivers(&self) -> (waivers: &[WaiverState])
        ensures waivers@ == self.spec_waivers(),
    { self.waivers.as_slice() }

    /// Returns whether the complete executable aggregate invariants hold.
    #[must_use]
    pub fn is_valid(&self) -> bool { validation::is_valid(self) }
}

impl Clone for KernelAggregate {
    fn clone(&self) -> (result: Self)
        ensures
            Self::scalar_clone_equivalent(self, &result),
            Self::sequence_clone_equivalent(self, &result),
            Self::action_clone_equivalent(self, &result),
            Self::clone_equivalent(self, &result),
            result.spec_revision() == self.spec_revision(),
            result.spec_reviews() == self.spec_reviews(),
            result.spec_waivers() == self.spec_waivers(),
    {
        let accepted_command_ids = self.accepted_command_ids.clone();
        let event_ids = self.event_ids.clone();
        let runs = self.runs.clone();
        let attempts = self.attempts.clone();
        let turns = self.turns.clone();
        let actions = ActionState::clone_sequence(self.actions.as_slice());
        let reviews = self.reviews.clone();
        let waivers = self.waivers.clone();
        proof {
            assert(accepted_command_ids@ =~= self.accepted_command_ids@);
            assert(event_ids@ =~= self.event_ids@);
            assert(runs@ =~= self.runs@);
            assert(attempts@ =~= self.attempts@);
            assert(turns@ =~= self.turns@);
            assert(reviews@ =~= self.reviews@);
            assert(waivers@ =~= self.waivers@);
        }
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

} // verus!
