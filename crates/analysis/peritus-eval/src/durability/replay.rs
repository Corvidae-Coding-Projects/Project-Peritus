//! Checked bounded C0 loading backed by the exact complete checkpoint.

use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::{DurableStateRecord, MAX_GLOBAL_WINDOW_RECORDS, SqliteJournal, StoreId};

use crate::{
    EvaluationCampaignId, EvaluationError, EvaluationErrorKind, EvaluationEvent,
    EvaluationOperation, EvaluationRecovery, EvaluationState,
    wire::{EvaluationEventFrame, EvaluationStateFrame},
};

use super::{EVALUATION_STATE_NAMESPACE, evaluation_aggregate_key, evaluation_state_key};

pub(super) struct CurrentEvaluationCheckpoint {
    pub(super) record: DurableStateRecord,
    pub(super) frame: EvaluationStateFrame,
}

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
    /// Returns the already validated exact checkpoint state.
    ///
    /// # Errors
    /// Rejects an impossible empty-history/checkpoint combination.
    pub fn rebuild(&self) -> Result<Option<EvaluationState>, EvaluationError> {
        match (&self.events[..], &self.checkpoint) {
            ([], None) => Ok(None),
            ([], Some(_)) => Err(recovery("evaluation checkpoint exists without immutable events")),
            (_, Some(checkpoint)) => Ok(Some(checkpoint.state().clone())),
            (_, None) => Err(recovery("evaluation events exist without an exact checkpoint")),
        }
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

/// Loads and verifies one evaluation aggregate from bounded C0 event windows.
///
/// The self-validating checkpoint is decoded once. Event windows then validate immutable record,
/// predecessor, revision, profile, and state-digest bindings without repeatedly materializing every
/// historical campaign state.
///
/// # Errors
/// Rejects journal failures, malformed frames, broken provenance, or checkpoint drift.
pub fn load_evaluation_replay(
    journal: &SqliteJournal,
    campaign_id: EvaluationCampaignId,
) -> Result<EvaluationReplay, EvaluationError> {
    let current = load_current_checkpoint(journal, campaign_id)?;
    let Some(current) = current else {
        return Ok(EvaluationReplay {
            store_id: journal.store_id(),
            events: Vec::new(),
            checkpoint: None,
        });
    };
    let aggregate = evaluation_aggregate_key(campaign_id)?;
    let state_key = evaluation_state_key(campaign_id);
    let expected_revision = peritus_evidence::revision_digest(current.frame.state().revision());
    let expected_profile = current.frame.state().profile_digest();
    let expected_sequence = current.frame.sequence();
    let mut events = Vec::new();
    let mut cursor = 0_u64;
    let mut previous_event = None;
    let mut previous_state_digest = peritus_types::Sha256Digest::new([0; 32]);
    loop {
        let records = journal
            .aggregate_events_after(aggregate, cursor, MAX_GLOBAL_WINDOW_RECORDS)
            .map_err(journal_error)?;
        if records.is_empty() {
            break;
        }
        events.try_reserve(records.len()).map_err(|_| recovery(
            "evaluation event projection cannot reserve its bounded window",
        ))?;
        for record in records {
            let frame = decode_message::<EvaluationEventFrame>(
                record.frame_bytes(),
                CodecLimits::PRODUCTION,
            )
            .map_err(codec)?;
            let event = frame.activate()?;
            let sequence = cursor.checked_add(1).ok_or_else(|| {
                recovery("evaluation event projection sequence overflowed")
            })?;
            if event.campaign_id() != campaign_id
                || event.sequence() != sequence
                || event.sequence() != record.sequence().get()
                || event.id() != record.event_id()
                || event.command_id() != record.command_id()
                || event.previous_event() != previous_event
                || event.previous_event() != record.previous_event_id()
                || event.prior_state_digest() != previous_state_digest
                || event.profile_digest() != expected_profile
                || record.revision_digest() != expected_revision
            {
                return Err(recovery(
                    "decoded evaluation event differs from its bounded C0 record chain",
                ));
            }
            let historical_root = journal
                .state_record_revision(EVALUATION_STATE_NAMESPACE, &state_key, sequence)
                .map_err(journal_error)?
                .ok_or_else(|| {
                    recovery("evaluation event has no immutable checkpoint root")
                })?;
            if historical_root.producing_position() != record.global_position() {
                return Err(recovery(
                    "evaluation event checkpoint was installed by another operation",
                ));
            }
            super::checkpoint::validate_event_root(
                &historical_root,
                campaign_id,
                &event,
            )?;
            cursor = sequence;
            previous_event = Some(event.id());
            previous_state_digest = event.successor_state_digest();
            events.push(event);
        }
        if cursor >= expected_sequence {
            break;
        }
    }
    if cursor != expected_sequence
        || previous_event != Some(current.frame.last_event_id())
        || previous_state_digest != current.frame.state_digest()
    {
        return Err(recovery(
            "evaluation checkpoint differs from the bounded immutable event frontier",
        ));
    }
    Ok(EvaluationReplay {
        store_id: journal.store_id(),
        events,
        checkpoint: Some(current.frame),
    })
}

pub(super) fn load_current_checkpoint(
    journal: &SqliteJournal,
    campaign_id: EvaluationCampaignId,
) -> Result<Option<CurrentEvaluationCheckpoint>, EvaluationError> {
    let aggregate = evaluation_aggregate_key(campaign_id)?;
    let state_key = evaluation_state_key(campaign_id);
    let head = journal.head(aggregate).map_err(journal_error)?;
    let record = journal
        .state_record(EVALUATION_STATE_NAMESPACE, &state_key)
        .map_err(journal_error)?;
    if head.is_some() != record.is_some() {
        return Err(recovery("evaluation event/checkpoint presence differs"));
    }
    let (Some(head), Some(record)) = (head, record) else {
        return Ok(None);
    };
    let frame = super::checkpoint::decode(journal, &record, campaign_id)?;
    if frame.sequence() != head.sequence().get()
        || frame.last_event_id() != head.event_id()
        || record.revision() != frame.sequence()
    {
        return Err(recovery("evaluation checkpoint differs from its aggregate head"));
    }
    Ok(Some(CurrentEvaluationCheckpoint { record, frame }))
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
