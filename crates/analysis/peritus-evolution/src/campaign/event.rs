//! Accepted campaign semantic events and successor checkpoints.

use crate::{CampaignCommandKind, CampaignState, EvolutionCampaignId};
use peritus_types::{CommandId, EventId, Sha256Digest};

/// Closed campaign event semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CampaignEventKind {
    /// One command was accepted with its complete restart-consumable semantics.
    Accepted(CampaignCommandKind),
}

/// One exact accepted campaign event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignEvent {
    id: EventId,
    command_id: CommandId,
    campaign_id: EvolutionCampaignId,
    sequence: u64,
    previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    policy_digest: Sha256Digest,
    command_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind: CampaignEventKind,
}

impl CampaignEvent {
    #[allow(clippy::too_many_arguments, reason = "all event integrity bindings remain explicit")]
    pub(crate) const fn from_replay_parts(
        id: EventId,
        command_id: CommandId,
        campaign_id: EvolutionCampaignId,
        sequence: u64,
        previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        policy_digest: Sha256Digest,
        command_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        kind: CampaignEventKind,
    ) -> Self {
        Self {
            id,
            command_id,
            campaign_id,
            sequence,
            previous_event,
            prior_state_digest,
            policy_digest,
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
    /// Aggregate identity.
    #[must_use]
    pub const fn campaign_id(&self) -> EvolutionCampaignId {
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
    /// Complete prior checkpoint digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Frozen typed-policy digest.
    #[must_use]
    pub const fn policy_digest(&self) -> Sha256Digest {
        self.policy_digest
    }
    /// Complete producing-command digest.
    #[must_use]
    pub const fn command_digest(&self) -> Sha256Digest {
        self.command_digest
    }
    /// Complete successor checkpoint digest.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Accepted semantic payload.
    #[must_use]
    pub const fn kind(&self) -> &CampaignEventKind {
        &self.kind
    }
}

/// One accepted event paired with its complete successor checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignTransition {
    event: CampaignEvent,
    state: CampaignState,
}

impl CampaignTransition {
    pub(crate) const fn new(event: CampaignEvent, state: CampaignState) -> Self {
        Self { event, state }
    }
    /// Accepted semantic event.
    #[must_use]
    pub const fn event(&self) -> &CampaignEvent {
        &self.event
    }
    /// Complete successor checkpoint.
    #[must_use]
    pub const fn state(&self) -> &CampaignState {
        &self.state
    }
    /// Consumes the transition.
    #[must_use]
    pub fn into_parts(self) -> (CampaignEvent, CampaignState) {
        (self.event, self.state)
    }
}
