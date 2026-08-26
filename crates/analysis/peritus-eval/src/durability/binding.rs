//! Stable C0 identities and cross-record transition validation.

use peritus_journal::{AggregateId, AggregateKey, AggregateKind};

use crate::{
    EvaluationCampaignId, EvaluationCommand, EvaluationError, EvaluationErrorKind,
    EvaluationEventKind, EvaluationOperation, EvaluationRecovery, EvaluationTransition,
};

/// Journal-owned namespace for complete E3 evaluation checkpoints.
pub const EVALUATION_STATE_NAMESPACE: u16 = 0xE301;
const STATE_KEY_DOMAIN: &[u8] = b"peritus.evaluation.state-key.v1\0";

/// Derives the dedicated C0 evaluation aggregate identity.
///
/// # Errors
/// Rejects an invalid campaign-to-aggregate identity conversion.
pub fn evaluation_aggregate_key(
    campaign_id: EvaluationCampaignId,
) -> Result<AggregateKey, EvaluationError> {
    let id = AggregateId::new(*campaign_id.as_bytes())
        .map_err(|_| journal("invalid C0 evaluation aggregate identity"))?;
    Ok(AggregateKey::new(AggregateKind::Evaluation, id))
}

/// Derives the domain-separated complete-checkpoint key.
#[must_use]
pub fn evaluation_state_key(campaign_id: EvaluationCampaignId) -> Vec<u8> {
    let mut key = Vec::with_capacity(STATE_KEY_DOMAIN.len() + campaign_id.as_bytes().len());
    key.extend_from_slice(STATE_KEY_DOMAIN);
    key.extend_from_slice(campaign_id.as_bytes());
    key
}

#[allow(
    clippy::suspicious_operation_groupings,
    reason = "the binding deliberately compares command, event, and checkpoint accessors with different names"
)]
pub(super) fn validate(
    command: &EvaluationCommand,
    transition: &EvaluationTransition,
) -> Result<(), EvaluationError> {
    let event = transition.event();
    let state = transition.state();
    let EvaluationEventKind::Accepted(kind) = event.kind();
    if (command.event_id() != event.id())
        || (command.command_id() != event.command_id())
        || (command.campaign_id() != event.campaign_id())
        || (command.campaign_id() != state.campaign_id())
        || (command.expected_previous_event() != event.previous_event())
        || (command.expected_sequence().checked_add(1) != Some(event.sequence()))
        || (command.prior_state_digest() != event.prior_state_digest())
        || (command.profile_digest() != event.profile_digest())
        || (command.profile_digest() != state.profile_digest())
        || (command.digest() != event.command_digest())
        || (event.successor_state_digest() != state.state_digest())
        || (event.sequence() != state.sequence())
        || (event.id() != state.last_event_id())
        || (command.kind() != kind)
    {
        return Err(binding("command, accepted event, and complete evaluation checkpoint differ"));
    }
    Ok(())
}

pub(super) const fn binding(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Binding,
        EvaluationOperation::Commit,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
pub(super) const fn journal(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Journal,
        EvaluationOperation::Commit,
        EvaluationRecovery::Replay,
        detail,
    )
}
