//! Reducer-driven campaign and pointer reconstruction from C0 records.

use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::{SqliteJournal, StoreId};
use peritus_types::EventSequence;

use crate::{
    CampaignEvent, CampaignState, EvolutionCampaignId, EvolutionError, PointerEvent,
    ProductionHarnessState, apply_campaign_event, apply_pointer_event,
    wire::{CampaignEventFrame, CampaignStateFrame, PointerEventFrame, PointerStateFrame},
};

use super::{
    CAMPAIGN_STATE_NAMESPACE, POINTER_STATE_NAMESPACE, campaign::codec, campaign::journal_error,
    campaign::recovery, campaign_aggregate_key, campaign_state_key, pointer_aggregate_key,
    pointer_state_key,
};

/// Fully replayed campaign observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignReplay {
    store_id: StoreId,
    events: Vec<CampaignEvent>,
    state: Option<CampaignState>,
}

impl CampaignReplay {
    /// Durable store that owns the observation.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }
    /// Complete ordered semantic event history.
    #[must_use]
    pub fn events(&self) -> &[CampaignEvent] {
        &self.events
    }
    /// Reconstructed current campaign, or absence for an unknown identity.
    #[must_use]
    pub const fn state(&self) -> Option<&CampaignState> {
        self.state.as_ref()
    }
}

/// Fully replayed production-pointer observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointerReplay {
    store_id: StoreId,
    events: Vec<PointerEvent>,
    state: Option<ProductionHarnessState>,
}

impl PointerReplay {
    /// Durable store that owns the observation.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }
    /// Complete ordered semantic pointer history.
    #[must_use]
    pub fn events(&self) -> &[PointerEvent] {
        &self.events
    }
    /// Reconstructed current pointer, or absence for an unknown project.
    #[must_use]
    pub const fn state(&self) -> Option<&ProductionHarnessState> {
        self.state.as_ref()
    }
}

/// Reconstructs one campaign and checks the complete checkpoint against reducer replay.
///
/// # Errors
/// Rejects journal failures, malformed frames, broken chains, or checkpoint disagreement.
pub fn recover_campaign(
    journal: &SqliteJournal,
    campaign_id: EvolutionCampaignId,
) -> Result<CampaignReplay, EvolutionError> {
    let aggregate = campaign_aggregate_key(campaign_id)?;
    let records = journal.records_for_aggregate(aggregate).map_err(journal_error)?;
    let checkpoint = journal
        .state_record(CAMPAIGN_STATE_NAMESPACE, &campaign_state_key(campaign_id))
        .map_err(journal_error)?;
    if records.is_empty() != checkpoint.is_none() {
        return Err(recovery("campaign events/checkpoint presence differs"));
    }
    let mut state = None;
    let mut events = Vec::with_capacity(records.len());
    for record in &records {
        let frame =
            decode_message::<CampaignEventFrame>(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(codec)?;
        let event = frame.check(state.as_ref())?;
        validate_record(record, event.sequence(), event.id(), event.previous_event())?;
        state = Some(apply_campaign_event(state.as_ref(), &event)?);
        events.push(event);
    }
    if let Some(record) = checkpoint {
        let frame = decode_message::<CampaignStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
            .map_err(codec)?;
        let observed = frame.into_state();
        let reconstructed =
            state.as_ref().ok_or_else(|| recovery("campaign checkpoint has no semantic events"))?;
        if record.revision() != reconstructed.sequence()
            || record.producing_position()
                != records.last().map_or(0, peritus_journal::CommittedRecord::global_position)
            || observed != *reconstructed
        {
            return Err(recovery("campaign checkpoint differs from replay"));
        }
    }
    Ok(CampaignReplay { store_id: journal.store_id(), events, state })
}

/// Reconstructs one production pointer and checks its checkpoint against reducer replay.
///
/// # Errors
/// Rejects journal failures, malformed frames, broken chains, or checkpoint disagreement.
pub fn recover_pointer(
    journal: &SqliteJournal,
    project_id: peritus_types::ProjectId,
) -> Result<PointerReplay, EvolutionError> {
    let aggregate = pointer_aggregate_key(project_id)?;
    let records = journal.records_for_aggregate(aggregate).map_err(journal_error)?;
    let checkpoint = journal
        .state_record(POINTER_STATE_NAMESPACE, &pointer_state_key(project_id))
        .map_err(journal_error)?;
    if records.is_empty() != checkpoint.is_none() {
        return Err(recovery("pointer events/checkpoint presence differs"));
    }
    let mut state = None;
    let mut events = Vec::with_capacity(records.len());
    for record in &records {
        let frame =
            decode_message::<PointerEventFrame>(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(codec)?;
        let event = frame.check(state.as_ref())?;
        validate_record(record, event.sequence(), event.id(), event.previous_event())?;
        state = Some(apply_pointer_event(state.as_ref(), &event)?);
        events.push(event);
    }
    if let Some(record) = checkpoint {
        let frame = decode_message::<PointerStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
            .map_err(codec)?;
        let observed = frame.into_state();
        let reconstructed =
            state.as_ref().ok_or_else(|| recovery("pointer checkpoint has no semantic events"))?;
        if record.revision() != reconstructed.sequence()
            || record.producing_position()
                != records.last().map_or(0, peritus_journal::CommittedRecord::global_position)
            || observed != *reconstructed
        {
            return Err(recovery("pointer checkpoint differs from replay"));
        }
    }
    Ok(PointerReplay { store_id: journal.store_id(), events, state })
}

fn validate_record(
    record: &peritus_journal::CommittedRecord,
    sequence: u64,
    event_id: peritus_types::EventId,
    previous: Option<peritus_types::EventId>,
) -> Result<(), EvolutionError> {
    let sequence =
        EventSequence::new(sequence).map_err(|_| recovery("decoded F0 event has zero sequence"))?;
    if record.sequence() != sequence
        || record.event_id() != event_id
        || record.previous_event_id() != previous
    {
        return Err(recovery("decoded F0 event differs from its C0 record"));
    }
    Ok(())
}
