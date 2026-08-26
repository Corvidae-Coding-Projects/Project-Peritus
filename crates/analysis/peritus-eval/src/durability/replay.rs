//! Checked C0 loading and exact complete-checkpoint replay.

use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::{SqliteJournal, StoreId};

use crate::{
    EvaluationCampaignId, EvaluationError, EvaluationErrorKind, EvaluationEvent,
    EvaluationOperation, EvaluationRecovery, EvaluationState, apply_event, replay,
    wire::{EvaluationEventFrame, EvaluationStateFrame},
};

use super::{EVALUATION_STATE_NAMESPACE, evaluation_aggregate_key, evaluation_state_key};

/// Contiguous family-86 events paired with the exact family-87 checkpoint.
pub struct EvaluationReplay {
    store_id: StoreId,
    events: Vec<EvaluationEvent>,
    checkpoint: Option<EvaluationStateFrame>,
}

impl EvaluationReplay {
    /// Durable store identity observed while loading.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }
    /// Contiguous checked semantic events.
    #[must_use]
    pub fn events(&self) -> &[EvaluationEvent] {
        &self.events
    }
    /// Rebuilds state and requires exact checkpoint equality.
    ///
    /// # Errors
    /// Rejects an incomplete history or any replay/checkpoint disagreement.
    pub fn rebuild(&self) -> Result<Option<EvaluationState>, EvaluationError> {
        if self.events.is_empty() {
            return if self.checkpoint.is_none() {
                Ok(None)
            } else {
                Err(recovery("evaluation checkpoint exists without immutable events"))
            };
        }
        let state = replay(&self.events)?;
        if !self.checkpoint.as_ref().is_some_and(|frame| frame.matches_state(&state)) {
            return Err(recovery("evaluation checkpoint differs from deterministic replay"));
        }
        Ok(Some(state))
    }
}

impl core::fmt::Debug for EvaluationReplay {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("EvaluationReplay")
            .field("store_id", &self.store_id)
            .field("events", &self.events.len())
            .field(
                "checkpoint_sequence",
                &self.checkpoint.as_ref().map(EvaluationStateFrame::sequence),
            )
            .finish_non_exhaustive()
    }
}

/// Loads and verifies one evaluation aggregate from C0.
///
/// # Errors
/// Rejects journal failures, malformed frames, broken provenance, or checkpoint drift.
pub fn load_evaluation_replay(
    journal: &SqliteJournal,
    campaign_id: EvaluationCampaignId,
) -> Result<EvaluationReplay, EvaluationError> {
    let aggregate = evaluation_aggregate_key(campaign_id)?;
    let state_key = evaluation_state_key(campaign_id);
    let records = journal.records_for_aggregate(aggregate).map_err(journal_error)?;
    let state_record =
        journal.state_record(EVALUATION_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    if records.is_empty() != state_record.is_none() {
        return Err(recovery("evaluation event/checkpoint presence differs"));
    }
    let mut events = Vec::with_capacity(records.len());
    let mut reconstructed: Option<EvaluationState> = None;
    for record in records {
        let frame =
            decode_message::<EvaluationEventFrame>(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(codec)?;
        let event = frame.check(reconstructed.as_ref())?;
        let successor = apply_event(reconstructed.as_ref(), &event)?;
        if event.campaign_id() != campaign_id
            || event.sequence() != record.sequence().get()
            || event.id() != record.event_id()
            || event.command_id() != record.command_id()
            || event.previous_event() != record.previous_event_id()
            || peritus_evidence::revision_digest(successor.revision()) != record.revision_digest()
        {
            return Err(recovery("decoded evaluation event differs from its C0 record"));
        }
        reconstructed = Some(successor);
        events.push(event);
    }
    let checkpoint = state_record
        .as_ref()
        .map(|record| {
            decode_message::<EvaluationStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
                .map_err(codec)
        })
        .transpose()?;
    if let Some(frame) = &checkpoint {
        let last = events.last().ok_or_else(|| recovery("checkpoint has no evaluation event"))?;
        let record = state_record.as_ref().ok_or_else(|| recovery("checkpoint vanished"))?;
        if frame.campaign_id() != campaign_id
            || frame.sequence() != last.sequence()
            || frame.last_event_id() != last.id()
            || frame.state_digest() != last.successor_state_digest()
            || record.revision() != frame.sequence()
            || !reconstructed.as_ref().is_some_and(|state| frame.matches_state(state))
        {
            return Err(recovery("evaluation checkpoint differs from aggregate head or replay"));
        }
    }
    Ok(EvaluationReplay { store_id: journal.store_id(), events, checkpoint })
}

fn codec(_: impl core::fmt::Display) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Corruption,
        EvaluationOperation::Codec,
        EvaluationRecovery::Quarantine,
        "evaluation journal frame violates canonical protocol",
    )
}
fn journal_error(_: impl core::fmt::Display) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Journal,
        EvaluationOperation::Recover,
        EvaluationRecovery::Replay,
        "C0 failed while loading evaluation replay",
    )
}
const fn recovery(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Recovery,
        EvaluationOperation::Recover,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
