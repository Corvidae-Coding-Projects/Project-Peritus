//! Canonical event drafts and state installs for atomic activation.

use peritus_codec::{CodecLimits, encode_message};
use peritus_journal::{EventDraft, ExactFrame, SqliteJournal, StateInstall};
use peritus_types::EventSequence;

use crate::{
    CampaignTransition, EvolutionError, EvolutionStorageLimits, PointerTransition,
    wire::{
        CampaignCommandFrame, CampaignEventFrame, PointerCommandFrame, PointerEventFrame,
    },
};

use super::super::{
    binding,
    campaign::{codec, journal_error}, checkpoint,
};

pub(super) fn campaign_event(
    aggregate: peritus_journal::AggregateKey,
    transition: &CampaignTransition,
    command: &CampaignCommandFrame,
) -> Result<EventDraft, EvolutionError> {
    let event = transition.event();
    let bytes = encode_message(
        &CampaignEventFrame::from_event_and_command(event, command).map_err(codec)?,
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
    command: &PointerCommandFrame,
) -> Result<EventDraft, EvolutionError> {
    let event = transition.event();
    let bytes = encode_message(
        &PointerEventFrame::from_event_and_command(event, command).map_err(codec)?,
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

pub(super) fn campaign_installs(
    journal: &SqliteJournal,
    key: Vec<u8>,
    expected_revision: u64,
    transition: &CampaignTransition,
    storage: EvolutionStorageLimits,
) -> Result<Vec<StateInstall>, EvolutionError> {
    checkpoint::campaign_compatible_installs(
        journal,
        &key,
        Some(expected_revision),
        transition.state(),
        storage,
    )
}

pub(super) fn pointer_installs(
    journal: &SqliteJournal,
    key: Vec<u8>,
    expected_revision: u64,
    transition: &PointerTransition,
    storage: EvolutionStorageLimits,
) -> Result<Vec<StateInstall>, EvolutionError> {
    checkpoint::pointer_compatible_installs(
        journal,
        &key,
        Some(expected_revision),
        transition.state(),
        storage,
    )
}
