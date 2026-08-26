//! Pure campaign decision, event application, and replay.

mod transition;

use crate::{
    CampaignCommand, CampaignCommandKind, CampaignEvent, CampaignEventKind, CampaignState,
    CampaignTransition, EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
};
use peritus_types::Sha256Digest;

/// Decides one campaign command without side effects.
///
/// # Errors
/// Rejects stale fences, terminal resurrection, illegal phases, evidence drift, duplicates, and
/// incomplete selection or promotion inputs.
pub fn decide_campaign(
    prior: Option<&CampaignState>,
    command: &CampaignCommand,
) -> Result<CampaignTransition, EvolutionError> {
    validate_fence(prior, command)?;
    let sequence = command.expected_sequence().checked_add(1).ok_or_else(transition::transition)?;
    let mut state = transition::apply_kind(
        prior,
        command.campaign_id(),
        sequence,
        command.event_id(),
        command.policy_digest(),
        command.kind(),
    )?;
    state.refresh_digest();
    let event = CampaignEvent::from_replay_parts(
        command.event_id(),
        command.command_id(),
        command.campaign_id(),
        sequence,
        command.expected_head(),
        command.prior_state_digest(),
        command.policy_digest(),
        command.digest(),
        state.state_digest(),
        CampaignEventKind::Accepted(command.kind().clone()),
    );
    Ok(CampaignTransition::new(event, state))
}

/// Applies one complete persisted event to its exact predecessor.
///
/// # Errors
/// Rejects a gap, immutable-binding drift, illegal semantics, or successor digest disagreement.
pub fn apply_campaign_event(
    prior: Option<&CampaignState>,
    event: &CampaignEvent,
) -> Result<CampaignState, EvolutionError> {
    let expected_sequence = prior.map_or(1, |state| state.sequence().saturating_add(1));
    let expected_head = prior.map(CampaignState::last_event);
    let expected_digest = prior.map_or(Sha256Digest::new([0; 32]), CampaignState::state_digest);
    if event.sequence() != expected_sequence
        || event.previous_event() != expected_head
        || event.prior_state_digest() != expected_digest
        || prior.is_some_and(|state| {
            state.campaign_id() != event.campaign_id()
                || state.policy().policy().digest() != event.policy_digest()
                || (state.phase().terminal()
                    && !matches!(
                        event.kind(),
                        CampaignEventKind::Accepted(CampaignCommandKind::RecordPublication(_))
                    ))
        })
    {
        return Err(stale());
    }
    let CampaignEventKind::Accepted(kind) = event.kind();
    let mut state = transition::apply_kind(
        prior,
        event.campaign_id(),
        event.sequence(),
        event.id(),
        event.policy_digest(),
        kind,
    )?;
    state.refresh_digest();
    if state.state_digest() != event.successor_state_digest() {
        return Err(corrupt("campaign event successor differs from pure replay"));
    }
    Ok(state)
}

/// Folds a nonempty contiguous campaign history.
///
/// # Errors
/// Returns the first invalid event or rejects an empty history.
pub fn replay_campaign(events: &[CampaignEvent]) -> Result<CampaignState, EvolutionError> {
    let mut state = None;
    for event in events {
        state = Some(apply_campaign_event(state.as_ref(), event)?);
    }
    state.ok_or_else(|| corrupt("campaign replay history is empty"))
}

fn validate_fence(
    prior: Option<&CampaignState>,
    command: &CampaignCommand,
) -> Result<(), EvolutionError> {
    match prior {
        None if command.expected_sequence() == 0
            && command.expected_head().is_none()
            && command.prior_state_digest() == Sha256Digest::new([0; 32])
            && matches!(command.kind(), CampaignCommandKind::CreateCampaign { .. }) =>
        {
            Ok(())
        }
        Some(state)
            if (!state.phase().terminal()
                || (state.publication().is_none()
                    && matches!(command.kind(), CampaignCommandKind::RecordPublication(_))))
                && command.expected_sequence() == state.sequence()
                && command.expected_head() == Some(state.last_event())
                && command.prior_state_digest() == state.state_digest()
                && command.campaign_id() == state.campaign_id()
                && command.policy_digest() == state.policy().policy().digest()
                && !matches!(command.kind(), CampaignCommandKind::CreateCampaign { .. }) =>
        {
            Ok(())
        }
        _ => Err(stale()),
    }
}

const fn stale() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::StaleState,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::RefreshState,
        "campaign command or event fence is stale",
    )
}

const fn corrupt(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Corruption,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::Quarantine,
        detail,
    )
}
