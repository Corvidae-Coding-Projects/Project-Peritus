//! Accepted evaluation events and successor-state transitions.

use peritus_types::{CommandId, EventId, Sha256Digest};

use crate::{EvaluationCampaignId, EvaluationState, ProfileDigest};

/// Closed family-86 semantic event vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvaluationEventKind {
    /// One command was accepted with the same exact inert semantics.
    Accepted(super::EvaluationCommandKind),
}

/// One fully bound accepted evaluation event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationEvent {
    id: EventId,
    command_id: CommandId,
    campaign_id: EvaluationCampaignId,
    sequence: u64,
    previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    profile_digest: ProfileDigest,
    command_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind: EvaluationEventKind,
}

impl EvaluationEvent {
    #[allow(clippy::too_many_arguments, reason = "all event integrity bindings remain explicit")]
    pub(crate) const fn new(
        id: EventId,
        command_id: CommandId,
        campaign_id: EvaluationCampaignId,
        sequence: u64,
        previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        profile_digest: ProfileDigest,
        command_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        kind: EvaluationEventKind,
    ) -> Self {
        Self {
            id,
            command_id,
            campaign_id,
            sequence,
            previous_event,
            prior_state_digest,
            profile_digest,
            command_digest,
            successor_state_digest,
            kind,
        }
    }
    /// Event identity.
    #[must_use]
    pub const fn id(&self) -> EventId {
        self.id
    }
    /// Producing command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Evaluation campaign identity.
    #[must_use]
    pub const fn campaign_id(&self) -> EvaluationCampaignId {
        self.campaign_id
    }
    /// Positive aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Exact predecessor event.
    #[must_use]
    pub const fn previous_event(&self) -> Option<EventId> {
        self.previous_event
    }
    /// Complete prior-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Immutable profile digest.
    #[must_use]
    pub const fn profile_digest(&self) -> ProfileDigest {
        self.profile_digest
    }
    /// Complete producing-command digest.
    #[must_use]
    pub const fn command_digest(&self) -> Sha256Digest {
        self.command_digest
    }
    /// Complete successor-state digest.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Accepted semantics.
    #[must_use]
    pub const fn kind(&self) -> &EvaluationEventKind {
        &self.kind
    }
}

/// Accepted event paired with its complete successor checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationTransition {
    event: EvaluationEvent,
    state: EvaluationState,
}

impl EvaluationTransition {
    pub(crate) const fn new(event: EvaluationEvent, state: EvaluationState) -> Self {
        Self { event, state }
    }
    /// Accepted event.
    #[must_use]
    pub const fn event(&self) -> &EvaluationEvent {
        &self.event
    }
    /// Complete successor state.
    #[must_use]
    pub const fn state(&self) -> &EvaluationState {
        &self.state
    }
    /// Consumes the transition.
    #[must_use]
    pub fn into_parts(self) -> (EvaluationEvent, EvaluationState) {
        (self.event, self.state)
    }
}
