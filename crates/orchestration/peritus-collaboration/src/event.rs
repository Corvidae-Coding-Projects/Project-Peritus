//! Immutable collaboration facts and complete successor transitions.

use peritus_types::{
    ActorId, CommandId, EventId, EventSequence, RevisionTuple, RunId, Sha256Digest,
};

use crate::{
    CancellationEffect, CollaborationBinding, CollaborationMessage, CollaborationMessageId,
    CollaborationState, CollaborationTaskId, Delegation, ReservationObservation, TaskTerminal,
};

/// Closed semantic fact accepted by the collaboration reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CollaborationEventKind {
    /// The aggregate and accepted root task were created.
    Started {
        /// Complete immutable collaboration binding.
        binding: CollaborationBinding,
    },
    /// A parent owner offered one child.
    DelegationOffered {
        /// Retained parent owner making the offer.
        offered_by: ActorId,
        /// Complete immutable child assignment.
        assignment: Delegation,
    },
    /// An assigned owner accepted one offer.
    DelegationAccepted {
        /// Offered task identity.
        task_id: CollaborationTaskId,
        /// Assigned owner accepting the offer.
        accepted_by: ActorId,
    },
    /// An assigned owner rejected one offer.
    DelegationRejected {
        /// Offered task identity.
        task_id: CollaborationTaskId,
        /// Assigned owner rejecting the offer.
        rejected_by: ActorId,
        /// Inert nonzero rejection-reason digest.
        reason_digest: Sha256Digest,
    },
    /// An accepted task became active under an exact scheduler reservation.
    TaskActivated {
        /// Accepted task identity.
        task_id: CollaborationTaskId,
        /// Exact scheduler reservation observation.
        observation: ReservationObservation,
    },
    /// An inert message became pending delivery.
    MessageSent {
        /// Complete inert causal message.
        message: CollaborationMessage,
    },
    /// The exact receiver acknowledged delivery.
    MessageAcknowledged {
        /// Pending message identity.
        message_id: CollaborationMessageId,
        /// Exact retained receiver.
        receiver: ActorId,
    },
    /// An owner retained one truthful terminal outcome.
    TaskCompleted {
        /// Active task identity.
        task_id: CollaborationTaskId,
        /// Retained owner reporting completion.
        completed_by: ActorId,
        /// Truthful terminal outcome.
        terminal: TaskTerminal,
    },
    /// Ownership ended without a success claim.
    TaskAbandoned {
        /// Accepted or active task identity.
        task_id: CollaborationTaskId,
        /// Retained owner ending ownership.
        abandoned_by: ActorId,
        /// Inert nonzero abandonment-reason digest.
        reason_digest: Sha256Digest,
    },
    /// Cancellation propagated in canonical task-identity order.
    CancellationPropagated {
        /// Root of the cancellation subtree.
        task_id: CollaborationTaskId,
        /// Task or ancestor owner requesting cancellation.
        requested_by: ActorId,
        /// Inert nonzero cancellation-reason digest.
        reason_digest: Sha256Digest,
        /// Canonical exact task-phase changes.
        effects: Vec<CancellationEffect>,
    },
    /// An active owner acknowledged cancellation.
    CancellationAcknowledged {
        /// Cancelling active task identity.
        task_id: CollaborationTaskId,
        /// Exact retained owner acknowledging termination.
        owner: ActorId,
    },
    /// New delegation was paused.
    Paused {
        /// Retained root owner requesting pause.
        requested_by: ActorId,
    },
    /// Delegation resumed.
    Resumed {
        /// Retained root owner requesting resume.
        requested_by: ActorId,
    },
    /// The aggregate committed its truthful terminal.
    Finalized,
}

/// One canonical event carrying exact predecessor and successor fences.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollaborationEvent {
    id: EventId,
    command_id: CommandId,
    sequence: EventSequence,
    previous_event: Option<EventId>,
    run_id: RunId,
    revision: RevisionTuple,
    prior_state_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind: CollaborationEventKind,
}

impl CollaborationEvent {
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire(
        id: EventId,
        command_id: CommandId,
        sequence: EventSequence,
        previous_event: Option<EventId>,
        run_id: RunId,
        revision: RevisionTuple,
        prior_state_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        kind: CollaborationEventKind,
    ) -> Self {
        Self {
            id,
            command_id,
            sequence,
            previous_event,
            run_id,
            revision,
            prior_state_digest,
            successor_state_digest,
            kind,
        }
    }
    /// Returns event identity.
    #[must_use]
    pub const fn id(&self) -> EventId {
        self.id
    }
    /// Returns causative command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns one-based sequence.
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    /// Returns exact causal predecessor.
    #[must_use]
    pub const fn previous_event(&self) -> Option<EventId> {
        self.previous_event
    }
    /// Returns run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns exact revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns predecessor-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns complete successor-state digest.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Borrows accepted semantic fact.
    #[must_use]
    pub const fn kind(&self) -> &CollaborationEventKind {
        &self.kind
    }
}

/// One accepted event plus complete successor state for atomic C0 commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollaborationTransition {
    event: CollaborationEvent,
    state: CollaborationState,
}

impl CollaborationTransition {
    pub(super) const fn new(event: CollaborationEvent, state: CollaborationState) -> Self {
        Self { event, state }
    }
    /// Borrows the immutable event.
    #[must_use]
    pub const fn event(&self) -> &CollaborationEvent {
        &self.event
    }
    /// Borrows the complete successor state.
    #[must_use]
    pub const fn state(&self) -> &CollaborationState {
        &self.state
    }
    /// Consumes the transition after durable commit.
    #[must_use]
    pub fn into_state(self) -> CollaborationState {
        self.state
    }
}
