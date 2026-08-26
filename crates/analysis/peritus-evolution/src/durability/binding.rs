//! Stable C0 identities and cross-record transition checks.

use peritus_journal::{AggregateId, AggregateKey, AggregateKind};
use peritus_types::ProjectId;

use crate::{
    CampaignCommand, CampaignEventKind, CampaignTransition, EvolutionCampaignId, EvolutionError,
    EvolutionErrorKind, EvolutionOperation, EvolutionRecovery, PointerCommand, PointerEventKind,
    PointerTransition,
};

/// Journal-owned namespace for complete campaign checkpoints.
pub const CAMPAIGN_STATE_NAMESPACE: u16 = 0xF001;
/// Journal-owned namespace for the complete production-pointer checkpoint.
pub const POINTER_STATE_NAMESPACE: u16 = 0xF002;

const CAMPAIGN_STATE_KEY_DOMAIN: &[u8] = b"peritus.evolution.campaign-state-key.v1\0";
const POINTER_STATE_KEY_DOMAIN: &[u8] = b"peritus.evolution.pointer-state-key.v1\0";

/// Derives the campaign aggregate key.
///
/// # Errors
/// Returns a typed journal binding error if the campaign identity cannot form a C0 key.
pub fn campaign_aggregate_key(
    campaign_id: EvolutionCampaignId,
) -> Result<AggregateKey, EvolutionError> {
    let id = AggregateId::new(*campaign_id.as_bytes())
        .map_err(|_| journal("invalid campaign aggregate identity"))?;
    Ok(AggregateKey::new(AggregateKind::EvolutionCampaign, id))
}

/// Derives the long-lived project pointer aggregate key.
///
/// # Errors
/// Returns a typed journal binding error if the project identity cannot form a C0 key.
pub fn pointer_aggregate_key(project_id: ProjectId) -> Result<AggregateKey, EvolutionError> {
    let id = AggregateId::new(*project_id.as_bytes())
        .map_err(|_| journal("invalid production-pointer aggregate identity"))?;
    Ok(AggregateKey::new(AggregateKind::ProductionHarness, id))
}

/// Derives the domain-separated campaign checkpoint key.
#[must_use]
pub fn campaign_state_key(campaign_id: EvolutionCampaignId) -> Vec<u8> {
    state_key(CAMPAIGN_STATE_KEY_DOMAIN, campaign_id.as_bytes())
}

/// Derives the domain-separated pointer checkpoint key.
#[must_use]
pub fn pointer_state_key(project_id: ProjectId) -> Vec<u8> {
    state_key(POINTER_STATE_KEY_DOMAIN, project_id.as_bytes())
}

fn state_key(domain: &[u8], id: &[u8; 16]) -> Vec<u8> {
    let mut key = Vec::with_capacity(domain.len() + id.len());
    key.extend_from_slice(domain);
    key.extend_from_slice(id);
    key
}

#[allow(clippy::suspicious_operation_groupings)]
pub(super) fn validate_campaign(
    command: &CampaignCommand,
    transition: &CampaignTransition,
) -> Result<(), EvolutionError> {
    let event = transition.event();
    let state = transition.state();
    let CampaignEventKind::Accepted(kind) = event.kind();
    if command.command_id() != event.command_id()
        || command.event_id() != event.id()
        || command.campaign_id() != event.campaign_id()
        || command.campaign_id() != state.campaign_id()
        || command.expected_head() != event.previous_event()
        || command.expected_sequence().checked_add(1) != Some(event.sequence())
        || command.prior_state_digest() != event.prior_state_digest()
        || command.policy_digest() != event.policy_digest()
        || command.policy_digest() != state.policy().policy().digest()
        || command.digest() != event.command_digest()
        || event.successor_state_digest() != state.state_digest()
        || event.sequence() != state.sequence()
        || event.id() != state.last_event()
        || command.kind() != kind
    {
        return Err(binding("campaign command, event, and checkpoint differ"));
    }
    Ok(())
}

#[allow(clippy::suspicious_operation_groupings)]
pub(super) fn validate_pointer(
    command: &PointerCommand,
    transition: &PointerTransition,
) -> Result<(), EvolutionError> {
    let event = transition.event();
    let state = transition.state();
    let PointerEventKind::Accepted(kind) = event.kind();
    if command.command_id() != event.command_id()
        || command.event_id() != event.id()
        || command.project_id() != event.project_id()
        || command.project_id() != state.project_id()
        || command.expected_head() != event.previous_event()
        || command.expected_sequence().checked_add(1) != Some(event.sequence())
        || command.expected_generation() != event.prior_generation()
        || event.successor_generation() != state.generation()
        || command.prior_state_digest() != event.prior_state_digest()
        || command.policy_digest() != event.policy_digest()
        || command.policy_digest() != state.policy().digest()
        || command.digest() != event.command_digest()
        || event.successor_state_digest() != state.state_digest()
        || event.sequence() != state.sequence()
        || event.id() != state.last_event()
        || command.kind() != kind
    {
        return Err(binding("pointer command, event, and checkpoint differ"));
    }
    Ok(())
}

pub(super) const fn binding(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::Commit,
        EvolutionRecovery::Quarantine,
        detail,
    )
}

pub(super) const fn journal(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Journal,
        EvolutionOperation::Commit,
        EvolutionRecovery::Replay,
        detail,
    )
}
