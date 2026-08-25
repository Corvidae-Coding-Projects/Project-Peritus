//! Closed pure D3 collaboration command vocabulary.

use peritus_types::{ActorId, CommandId, EventId, RevisionTuple, RunId, Sha256Digest};

use crate::{
    CollaborationBinding, CollaborationMessage, CollaborationMessageId, CollaborationTaskId,
    Delegation, ReservationObservation, TaskTerminal,
};

/// Core semantic payload of one fenced collaboration command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CollaborationCommandKind {
    /// Starts one collaboration aggregate with its accepted root assignment.
    Start {
        /// Complete immutable collaboration binding.
        binding: CollaborationBinding,
    },
    /// Parent owner offers one causally bound child assignment.
    OfferDelegation {
        /// Retained parent owner making the offer.
        offered_by: ActorId,
        /// Complete immutable child assignment.
        assignment: Delegation,
    },
    /// Assigned owner accepts an offered child.
    AcceptDelegation {
        /// Offered task identity.
        task_id: CollaborationTaskId,
        /// Assigned owner accepting the offer.
        accepted_by: ActorId,
    },
    /// Assigned owner rejects an offered child.
    RejectDelegation {
        /// Offered task identity.
        task_id: CollaborationTaskId,
        /// Assigned owner rejecting the offer.
        rejected_by: ActorId,
        /// Inert nonzero rejection-reason digest.
        reason_digest: Sha256Digest,
    },
    /// Activates an accepted task after observing its exact scheduler reservation.
    ActivateTask {
        /// Accepted task identity.
        task_id: CollaborationTaskId,
        /// Exact scheduler reservation observation.
        observation: ReservationObservation,
    },
    /// Retains one bounded inert causal message pending receiver acknowledgement.
    SendMessage {
        /// Complete inert causal message.
        message: CollaborationMessage,
    },
    /// Acknowledges delivery by the exact retained receiver.
    AcknowledgeMessage {
        /// Pending message identity.
        message_id: CollaborationMessageId,
        /// Exact retained receiver.
        receiver: ActorId,
    },
    /// Completes active work with a truthful terminal outcome.
    CompleteTask {
        /// Active task identity.
        task_id: CollaborationTaskId,
        /// Retained owner reporting completion.
        completed_by: ActorId,
        /// Truthful terminal outcome.
        terminal: TaskTerminal,
    },
    /// Abandons accepted or active ownership without manufacturing success.
    AbandonTask {
        /// Accepted or active task identity.
        task_id: CollaborationTaskId,
        /// Retained owner ending ownership.
        abandoned_by: ActorId,
        /// Inert nonzero abandonment-reason digest.
        reason_digest: Sha256Digest,
    },
    /// Propagates cancellation through the named task and every descendant.
    CancelTask {
        /// Root of the cancellation subtree.
        task_id: CollaborationTaskId,
        /// Task or ancestor owner requesting cancellation.
        requested_by: ActorId,
        /// Inert nonzero cancellation-reason digest.
        reason_digest: Sha256Digest,
    },
    /// Active owner acknowledges termination after propagated cancellation.
    AcknowledgeCancellation {
        /// Cancelling active task identity.
        task_id: CollaborationTaskId,
        /// Exact retained owner acknowledging termination.
        owner: ActorId,
    },
    /// Pauses new delegation while preserving active ownership and delivery facts.
    Pause {
        /// Retained root owner requesting pause.
        requested_by: ActorId,
    },
    /// Resumes delegation.
    Resume {
        /// Retained root owner requesting resume.
        requested_by: ActorId,
    },
    /// Computes truthful aggregate terminal state.
    Finalize,
}

/// One syntax-checked but unprivileged fenced collaboration command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollaborationCommand {
    command_id: CommandId,
    event_id: EventId,
    run_id: RunId,
    expected_sequence: u64,
    expected_previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    revision: RevisionTuple,
    kind: CollaborationCommandKind,
}

impl CollaborationCommand {
    /// Creates a command with exact genesis/non-genesis predecessor shape.
    ///
    /// # Errors
    /// Rejects inconsistent sequence/predecessor shape.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command_id: CommandId,
        event_id: EventId,
        run_id: RunId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        revision: RevisionTuple,
        kind: CollaborationCommandKind,
    ) -> Result<Self, crate::CollaborationError> {
        if (expected_sequence == 0) != expected_previous_event.is_none() {
            return Err(crate::error::reject(
                crate::CollaborationErrorKind::StaleFence,
                "command predecessor shape is inconsistent",
            ));
        }
        Ok(Self::from_wire(
            command_id,
            event_id,
            run_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            revision,
            kind,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire(
        command_id: CommandId,
        event_id: EventId,
        run_id: RunId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        revision: RevisionTuple,
        kind: CollaborationCommandKind,
    ) -> Self {
        Self {
            command_id,
            event_id,
            run_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            revision,
            kind,
        }
    }

    /// Returns the idempotent command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns the reserved successor event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns the run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns the expected predecessor sequence.
    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    /// Returns the expected predecessor event.
    #[must_use]
    pub const fn expected_previous_event(&self) -> Option<EventId> {
        self.expected_previous_event
    }
    /// Returns the expected predecessor state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns the immutable revision fence.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Borrows the closed semantic command.
    #[must_use]
    pub const fn kind(&self) -> &CollaborationCommandKind {
        &self.kind
    }
}
