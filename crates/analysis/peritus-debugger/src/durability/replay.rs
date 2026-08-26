//! Checked C0 loading and exact complete-checkpoint recovery.

use core::fmt;

use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::{SqliteJournal, StoreId};

use crate::{
    DebuggerError, DebuggerErrorKind, DebuggerEvent, DebuggerJobId, DebuggerOperation,
    DebuggerRecovery, DebuggerState, apply_event, replay,
    wire::{DebuggerEventFrame, DebuggerStateFrame},
};

use super::{DEBUGGER_STATE_NAMESPACE, debugger_aggregate_key, debugger_state_key};

/// Contiguous canonical debugger events paired with the exact atomic checkpoint.
pub struct DebuggerReplay {
    store_id: StoreId,
    events: Vec<DebuggerEvent>,
    checkpoint: Option<DebuggerStateFrame>,
}

impl DebuggerReplay {
    /// Durable store identity observed during loading.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }
    /// Contiguous checked semantic events.
    #[must_use]
    pub fn events(&self) -> &[DebuggerEvent] {
        &self.events
    }

    /// Deterministically rebuilds state and requires exact checkpoint equality.
    ///
    /// # Errors
    /// Rejects illegal history or any absent/ahead/behind/different checkpoint.
    pub fn rebuild(&self) -> Result<Option<DebuggerState>, DebuggerError> {
        if self.events.is_empty() {
            return if self.checkpoint.is_none() {
                Ok(None)
            } else {
                Err(recovery("debugger checkpoint exists without immutable events"))
            };
        }
        let state = replay(&self.events)?;
        if !self.checkpoint.as_ref().is_some_and(|frame| frame.matches_state(&state)) {
            return Err(recovery("debugger checkpoint differs from deterministic replay"));
        }
        Ok(Some(state))
    }
}

impl fmt::Debug for DebuggerReplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DebuggerReplay")
            .field("store_id", &self.store_id)
            .field("events", &self.events.len())
            .field(
                "checkpoint_sequence",
                &self.checkpoint.as_ref().map(DebuggerStateFrame::sequence),
            )
            .finish_non_exhaustive()
    }
}

/// Loads a debugger job's family-83 events and family-84 checkpoint from C0.
///
/// # Errors
/// Rejects gaps, wrong families, record/frame mismatch, invalid semantics, or divergence.
pub fn load_debugger_replay(
    journal: &SqliteJournal,
    job_id: DebuggerJobId,
) -> Result<DebuggerReplay, DebuggerError> {
    let aggregate = debugger_aggregate_key(job_id)?;
    let state_key = debugger_state_key(job_id);
    let records = journal.records_for_aggregate(aggregate).map_err(journal_error)?;
    let state_record =
        journal.state_record(DEBUGGER_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    if records.is_empty() != state_record.is_none() {
        return Err(recovery("debugger event/checkpoint presence differs"));
    }
    let mut events = Vec::with_capacity(records.len());
    let mut reconstructed: Option<DebuggerState> = None;
    for record in records {
        let frame =
            decode_message::<DebuggerEventFrame>(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(codec)?;
        let event = frame.check(reconstructed.as_ref())?;
        let successor = apply_event(reconstructed.as_ref(), &event)?;
        if event.job_id() != job_id
            || event.sequence() != record.sequence().get()
            || event.id() != record.event_id()
            || event.command_id() != record.command_id()
            || event.previous_event() != record.previous_event_id()
            || peritus_evidence::revision_digest(successor.revision()) != record.revision_digest()
        {
            return Err(recovery("decoded debugger event differs from its C0 record"));
        }
        reconstructed = Some(successor);
        events.push(event);
    }
    let checkpoint = state_record
        .as_ref()
        .map(|record| {
            decode_message::<DebuggerStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
                .map_err(codec)
        })
        .transpose()?;
    if let Some(frame) = &checkpoint {
        let last = events.last().ok_or_else(|| recovery("debugger checkpoint has no event"))?;
        let record =
            state_record.as_ref().ok_or_else(|| recovery("debugger checkpoint vanished"))?;
        if frame.job_id() != job_id
            || frame.sequence() != last.sequence()
            || frame.last_event_id() != last.id()
            || frame.state_digest() != last.successor_state_digest()
            || record.revision() != frame.sequence()
            || !reconstructed.as_ref().is_some_and(|state| frame.matches_state(state))
        {
            return Err(recovery("debugger checkpoint differs from aggregate head or replay"));
        }
    }
    Ok(DebuggerReplay { store_id: journal.store_id(), events, checkpoint })
}

fn codec(error: impl fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Corruption,
        DebuggerOperation::DecodeProtocol,
        DebuggerRecovery::Quarantine,
        error.to_string(),
    )
}

fn journal_error(error: impl fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Journal,
        DebuggerOperation::Replay,
        DebuggerRecovery::ReplayAggregate,
        error.to_string(),
    )
}

fn recovery(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Recovery,
        DebuggerOperation::Recover,
        DebuggerRecovery::Quarantine,
        detail,
    )
}
