//! Canonical event drafts and state installs for atomic activation.

use peritus_codec::{CodecLimits, encode_message};
use peritus_journal::{EventDraft, ExactFrame, StateInstall};
use peritus_types::EventSequence;

use crate::{
    CampaignTransition, EvolutionError, PointerTransition,
    wire::{CampaignEventFrame, CampaignStateFrame, PointerEventFrame, PointerStateFrame},
};

use super::super::{
    CAMPAIGN_STATE_NAMESPACE, POINTER_STATE_NAMESPACE, binding,
    campaign::{codec, journal_error},
};

pub(super) fn campaign_event(
    aggregate: peritus_journal::AggregateKey,
    transition: &CampaignTransition,
) -> Result<EventDraft, EvolutionError> {
    let event = transition.event();
    let bytes = encode_message(
        &CampaignEventFrame::from_event(event).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    EventDraft::new(
        aggregate,
        EventSequence::new(event.sequence())
            .map_err(|_| binding::binding("zero campaign event"))?,
        event.id(),
        event.previous_event(),
        ExactFrame::new(bytes).map_err(journal_error)?,
        transition.state().state_digest(),
        Vec::new(),
    )
    .map_err(journal_error)
}

pub(super) fn pointer_event(
    aggregate: peritus_journal::AggregateKey,
    transition: &PointerTransition,
) -> Result<EventDraft, EvolutionError> {
    let event = transition.event();
    let bytes = encode_message(
        &PointerEventFrame::from_event(event).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    EventDraft::new(
        aggregate,
        EventSequence::new(event.sequence()).map_err(|_| binding::binding("zero pointer event"))?,
        event.id(),
        event.previous_event(),
        ExactFrame::new(bytes).map_err(journal_error)?,
        super::super::pointer::pointer_event_revision_digest(transition.state()),
        Vec::new(),
    )
    .map_err(journal_error)
}

pub(super) fn campaign_install(
    key: Vec<u8>,
    expected_revision: u64,
    transition: &CampaignTransition,
) -> Result<StateInstall, EvolutionError> {
    let bytes = encode_message(
        &CampaignStateFrame::from_state(transition.state()).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    StateInstall::new(
        CAMPAIGN_STATE_NAMESPACE,
        key,
        Some(expected_revision),
        transition.state().sequence(),
        bytes,
    )
    .map_err(journal_error)
}

pub(super) fn pointer_install(
    key: Vec<u8>,
    expected_revision: u64,
    transition: &PointerTransition,
) -> Result<StateInstall, EvolutionError> {
    let bytes = encode_message(
        &PointerStateFrame::from_state(transition.state()).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    StateInstall::new(
        POINTER_STATE_NAMESPACE,
        key,
        Some(expected_revision),
        transition.state().sequence(),
        bytes,
    )
    .map_err(journal_error)
}
