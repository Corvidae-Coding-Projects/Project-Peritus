//! Reducer-driven campaign and pointer reconstruction from C0 records.

use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::{SqliteJournal, StoreId};
use peritus_types::EventSequence;

use crate::{
    CampaignEvent, CampaignState, DurableActivationHistory, DurableActivationOrigin,
    EvolutionCampaignId, EvolutionError, PendingActivation, PointerCommandKind, PointerEvent,
    PointerEventKind, ProductionHarnessState, apply_campaign_event, apply_pointer_event,
    wire::{CampaignEventFrame, PointerEventFrame},
};
use crate::pointer::apply_pointer_event_legacy_eviction;

use super::{
    CAMPAIGN_STATE_NAMESPACE, POINTER_STATE_NAMESPACE, campaign::codec, campaign::journal_error,
    campaign::recovery, campaign_aggregate_key, campaign_state_key, checkpoint,
    pointer_aggregate_key, pointer_state_key,
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
    activation_history: DurableActivationHistory,
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
    /// Complete activation ledger reconstructed from immutable event/checkpoint origins.
    #[must_use]
    pub const fn activation_history(&self) -> &DurableActivationHistory {
        &self.activation_history
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
        let frame = checkpoint::decode_campaign(journal, &record, campaign_id)?;
        let reconstructed =
            state.as_ref().ok_or_else(|| recovery("campaign checkpoint has no semantic events"))?;
        if record.revision() != reconstructed.sequence() {
            return Err(recovery("campaign checkpoint revision differs from replay"));
        }
        let last =
            records.last().ok_or_else(|| recovery("campaign checkpoint has no producing event"))?;
        if !checkpoint_producer_matches(journal, &record, last)? {
            return Err(recovery("campaign checkpoint position differs from replay"));
        }
        if !frame.matches_state(reconstructed) {
            return Err(recovery("campaign checkpoint state differs from replay"));
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
    let state_key = pointer_state_key(project_id);
    let checkpoint = journal
        .state_record(POINTER_STATE_NAMESPACE, &state_key)
        .map_err(journal_error)?;
    if records.is_empty() != checkpoint.is_none() {
        return Err(recovery("pointer events/checkpoint presence differs"));
    }
    let mut state = None;
    let mut events = Vec::with_capacity(records.len());
    let mut activation_history = DurableActivationHistory::empty(journal.store_id(), project_id);
    for record in &records {
        let frame =
            decode_message::<PointerEventFrame>(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(codec)?;
        let event = frame.into_event()?;
        validate_record(record, event.sequence(), event.id(), event.previous_event())?;
        let historical = journal
            .state_record_revision(POINTER_STATE_NAMESPACE, &state_key, event.sequence())
            .map_err(journal_error)?
            .ok_or_else(|| recovery("pointer event has no immutable checkpoint revision"))?;
        let observed_checkpoint = checkpoint::decode_pointer(journal, &historical, project_id)?;
        validate_pointer_checkpoint(
            journal,
            record,
            &event,
            &historical,
            &observed_checkpoint,
        )?;
        validate_durable_rollback_event(&activation_history, state.as_ref(), &event)?;

        let reconstructed = if let Some(observed) = observed_checkpoint.legacy_state() {
            let candidate = match apply_pointer_event(state.as_ref(), &event) {
                Ok(candidate) if candidate == *observed => candidate,
                _ if legacy_eviction_shape(state.as_ref(), &event, observed) => {
                    apply_pointer_event_legacy_eviction(state.as_ref(), &event)?
                }
                _ => return Err(recovery("pointer checkpoint differs from pure event replay")),
            };
            if candidate != *observed {
                return Err(recovery(
                    "legacy pointer checkpoint differs from exact eviction replay",
                ));
            }
            candidate
        } else {
            let candidate = apply_pointer_event(state.as_ref(), &event)?;
            if !observed_checkpoint.matches_state(&candidate) {
                return Err(recovery("pointer checkpoint reference differs from event replay"));
            }
            candidate
        };

        if event.successor_generation() != event.prior_generation() {
            let activation = reconstructed
                .history()
                .last()
                .cloned()
                .ok_or_else(|| recovery("activation event checkpoint has no activation record"))?;
            let origin = DurableActivationOrigin::from_committed(
                journal.store_id(),
                project_id,
                activation,
                event.sequence(),
                event.id(),
                event.command_id(),
                record.global_position(),
                record.previous_event_hash(),
                record.event_hash(),
                record.frame_digest(),
                record.revision_digest(),
                event.successor_state_digest(),
                historical.digest(),
                historical.producing_position(),
            )?;
            activation_history.push(origin)?;
        }
        if !activation_history.matches_state(&reconstructed) {
            return Err(recovery(
                "pointer checkpoint cache is not a suffix of durable activation history",
            ));
        }
        state = Some(reconstructed);
        events.push(event);
    }
    if let Some(record) = checkpoint {
        let frame = checkpoint::decode_pointer(journal, &record, project_id)?;
        let reconstructed =
            state.as_ref().ok_or_else(|| recovery("pointer checkpoint has no semantic events"))?;
        let last =
            records.last().ok_or_else(|| recovery("pointer checkpoint has no producing event"))?;
        let historical = journal
            .state_record_revision(POINTER_STATE_NAMESPACE, &state_key, reconstructed.sequence())
            .map_err(journal_error)?
            .ok_or_else(|| recovery("current pointer has no immutable checkpoint revision"))?;
        if record.revision() != reconstructed.sequence()
            || !checkpoint_producer_matches(journal, &record, last)?
            || record.digest() != historical.digest()
            || record.producing_position() != historical.producing_position()
            || !frame.matches_state(reconstructed)
        {
            return Err(recovery("pointer checkpoint differs from replay"));
        }
    }
    Ok(PointerReplay {
        store_id: journal.store_id(),
        events,
        state,
        activation_history,
    })
}

fn validate_pointer_checkpoint(
    journal: &SqliteJournal,
    record: &peritus_journal::CommittedRecord,
    event: &PointerEvent,
    checkpoint: &peritus_journal::DurableStateRecord,
    observed: &checkpoint::PointerCheckpoint,
) -> Result<(), EvolutionError> {
    if checkpoint.revision() != event.sequence()
        || !checkpoint_producer_matches(journal, checkpoint, record)?
        || observed.project_id() != event.project_id()
        || observed.sequence() != event.sequence()
        || observed.last_event_id() != event.id()
        || observed.generation() != event.successor_generation()
        || observed.policy_digest() != event.policy_digest()
        || observed.state_digest() != event.successor_state_digest()
        || record.revision_digest() != observed.revision_digest()
    {
        return Err(recovery(
            "pointer event, committed origin, and historical checkpoint differ",
        ));
    }
    Ok(())
}

fn validate_durable_rollback_event(
    history: &DurableActivationHistory,
    prior: Option<&ProductionHarnessState>,
    event: &PointerEvent,
) -> Result<(), EvolutionError> {
    let PointerEventKind::Accepted(kind) = event.kind();
    let valid = match kind {
        PointerCommandKind::PrepareRollback(proposal) => history.validates_proposal(proposal),
        PointerCommandKind::ActivateRollback { rollback_id, .. } => {
            matches!(
                prior.and_then(|state| state.pending()),
                Some(PendingActivation::Rollback(proposal))
                    if proposal.id() == *rollback_id && history.validates_proposal(proposal)
            )
        }
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(recovery(
            "rollback event target has no matching immutable activation origin",
        ))
    }
}

fn legacy_eviction_shape(
    prior: Option<&ProductionHarnessState>,
    event: &PointerEvent,
    observed: &ProductionHarnessState,
) -> bool {
    let Some(prior) = prior else {
        return false;
    };
    let Some(limit) = prior.limits().activation_history_limit().map(usize::from) else {
        return false;
    };
    let PointerEventKind::Accepted(kind) = event.kind();
    let activation = matches!(
        kind,
        PointerCommandKind::ActivatePromotion { .. }
            | PointerCommandKind::ActivateRollback { .. }
    );
    if !activation
        || prior.history().len() != limit
        || observed.history().len() != limit
        || prior.generation().checked_add(1) != Some(observed.generation())
        || event.successor_generation() != observed.generation()
    {
        return false;
    }
    let retained = &prior.history()[1..];
    observed.history().get(..retained.len()) == Some(retained)
}

fn checkpoint_producer_matches(
    journal: &SqliteJournal,
    checkpoint: &peritus_journal::DurableStateRecord,
    aggregate_event: &peritus_journal::CommittedRecord,
) -> Result<bool, EvolutionError> {
    let position = checkpoint.producing_position();
    if position < aggregate_event.global_position() {
        return Ok(false);
    }
    let producer =
        journal.global_events_after(position.saturating_sub(1), 1).map_err(journal_error)?;
    Ok(producer.records().first().is_some_and(|record| {
        record.global_position() == position && record.command_id() == aggregate_event.command_id()
    }))
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
