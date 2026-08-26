//! Checked C0 loading and exact complete-checkpoint recovery.

use core::fmt;

use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::{SqliteJournal, StoreId};
use peritus_types::HarnessId;

use crate::{
    aggregate::{HarnessEvent, HarnessState, apply_event},
    wire::{HarnessEventFrame, HarnessStateFrame},
};

use super::{
    DurabilityError, DurabilityErrorKind, DurabilityRecovery, HARNESS_STATE_NAMESPACE,
    harness_aggregate_key, harness_state_key,
};

/// Contiguous canonical harness events plus their exact atomic checkpoint.
pub struct HarnessReplay {
    store_id: StoreId,
    events: Vec<HarnessEvent>,
    checkpoint: Option<HarnessStateFrame>,
}

impl HarnessReplay {
    /// Returns durable store identity.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }
    /// Borrows contiguous checked semantic events.
    #[must_use]
    pub fn events(&self) -> &[HarnessEvent] {
        &self.events
    }

    /// Deterministically rebuilds state and requires exact complete-checkpoint equality.
    ///
    /// # Errors
    /// Rejects illegal history, absent/ahead/behind/different state, or terminal divergence.
    pub fn rebuild(&self) -> Result<Option<HarnessState>, DurabilityError> {
        if self.events.is_empty() {
            return if self.checkpoint.is_none() {
                Ok(None)
            } else {
                Err(recovery("harness checkpoint exists without immutable events"))
            };
        }
        let state = crate::replay::replay(&self.events).map_err(aggregate)?;
        if !self.checkpoint.as_ref().is_some_and(|frame| frame.matches_state(&state)) {
            return Err(recovery("harness checkpoint differs from deterministic replay"));
        }
        Ok(Some(state))
    }
}

impl fmt::Debug for HarnessReplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HarnessReplay")
            .field("store_id", &self.store_id)
            .field("events", &self.events.len())
            .field(
                "checkpoint_sequence",
                &self.checkpoint.as_ref().map(HarnessStateFrame::sequence),
            )
            .finish_non_exhaustive()
    }
}

/// Loads one harness's canonical events and complete current checkpoint from C0.
///
/// # Errors
/// Rejects gaps, wrong frame families, record/frame mismatch, invalid semantics, and
/// head/checkpoint divergence.
pub fn load_harness_replay(
    journal: &SqliteJournal,
    harness_id: HarnessId,
) -> Result<HarnessReplay, DurabilityError> {
    let aggregate_key = harness_aggregate_key(harness_id)?;
    let state_key = harness_state_key(harness_id);
    let records = journal.records_for_aggregate(aggregate_key).map_err(journal_error)?;
    let state_record =
        journal.state_record(HARNESS_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    if records.is_empty() != state_record.is_none() {
        return Err(recovery("harness events/checkpoint presence differs"));
    }
    let mut events = Vec::with_capacity(records.len());
    let mut reconstructed: Option<HarnessState> = None;
    for record in records {
        let frame =
            decode_message::<HarnessEventFrame>(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(codec)?;
        let event = frame.check(reconstructed.as_ref()).map_err(aggregate)?;
        if event.harness_id() != harness_id
            || event.sequence() != record.sequence().get()
            || event.id() != record.event_id()
            || event.command_id() != record.command_id()
            || event.previous_event() != record.previous_event_id()
            || event.revision_digest().digest() != record.revision_digest()
        {
            return Err(recovery("decoded harness event differs from its C0 record"));
        }
        reconstructed = Some(apply_event(reconstructed.as_ref(), &event).map_err(aggregate)?);
        events.push(event);
    }
    let checkpoint = state_record
        .as_ref()
        .map(|record| {
            decode_message::<HarnessStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
                .map_err(codec)
        })
        .transpose()?;
    if let Some(frame) = &checkpoint {
        let last = events.last().ok_or_else(|| recovery("harness checkpoint has no event"))?;
        let record =
            state_record.as_ref().ok_or_else(|| recovery("harness checkpoint vanished"))?;
        if frame.harness_id() != harness_id
            || frame.sequence() != last.sequence()
            || frame.last_event_id() != last.id()
            || frame.state_digest() != last.successor_state_digest()
            || record.revision() != frame.sequence()
            || !reconstructed.as_ref().is_some_and(|state| frame.matches_state(state))
        {
            return Err(recovery("harness checkpoint differs from aggregate head or replay"));
        }
    }
    Ok(HarnessReplay { store_id: journal.store_id(), events, checkpoint })
}

fn codec(error: impl fmt::Display) -> DurabilityError {
    DurabilityError::new(
        DurabilityErrorKind::Codec,
        DurabilityRecovery::Quarantine,
        error.to_string(),
    )
}
fn journal_error(error: impl fmt::Display) -> DurabilityError {
    DurabilityError::new(
        DurabilityErrorKind::Journal,
        DurabilityRecovery::ReplayAggregate,
        error.to_string(),
    )
}
fn aggregate(error: impl fmt::Display) -> DurabilityError {
    DurabilityError::new(
        DurabilityErrorKind::Recovery,
        DurabilityRecovery::Quarantine,
        error.to_string(),
    )
}
fn recovery(detail: &'static str) -> DurabilityError {
    DurabilityError::new(DurabilityErrorKind::Recovery, DurabilityRecovery::Quarantine, detail)
}
