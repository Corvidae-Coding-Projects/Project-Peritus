//! Checked replay from journal genesis.

use crate::{
    Checkpoint, FoldContext, Projection, ProjectionError, ProjectionErrorKind, ProjectionState,
    RecoveryClass,
};
use peritus_codec::{CodecLimits, decode_frame};
use peritus_journal::{AggregateKey, IntegrityExport};
use peritus_protocol::schema::FAMILIES;
use peritus_types::{EventId, Sha256Digest};
use std::collections::BTreeMap;

/// Successful pure replay result and its deterministic payload.
#[derive(Debug)]
pub struct ReplayOutput<S> {
    state: S,
    payload: Vec<u8>,
    checkpoint: Checkpoint,
    invariant_digest: Sha256Digest,
    record_count: u64,
}

impl<S> ReplayOutput<S> {
    /// Borrows the completed in-memory state.
    #[must_use]
    pub const fn state(&self) -> &S {
        &self.state
    }

    /// Consumes the output and returns the completed state.
    #[must_use]
    pub fn into_state(self) -> S {
        self.state
    }

    /// Borrows the deterministic encoded payload.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Borrows the journal- and schema-bound checkpoint.
    #[must_use]
    pub const fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }

    /// Returns the independently computed invariant checksum.
    #[must_use]
    pub const fn invariant_digest(&self) -> Sha256Digest {
        self.invariant_digest
    }

    /// Returns the number of records consumed.
    #[must_use]
    pub const fn record_count(&self) -> u64 {
        self.record_count
    }
}

#[derive(Clone, Copy)]
struct AggregateCursor {
    sequence: u64,
    event_id: EventId,
    event_hash: Sha256Digest,
    revision: Sha256Digest,
}

/// Replays an integrity-checked exact journal export from genesis with no external effects.
///
/// # Errors
///
/// Rejects range mismatches, gaps, order violations, unknown families, unsupported schemas,
/// aggregate revision changes, typed fold failures, and final invariant failures.
pub fn replay_from_genesis<P: Projection>(
    projection: &P,
    export: &IntegrityExport,
) -> Result<ReplayOutput<P::State>, ProjectionError> {
    validate_export_range(export)?;
    let mut state = projection.genesis();
    let mut last_position = 0_u64;
    let mut aggregates = BTreeMap::<AggregateKey, AggregateCursor>::new();
    for record in export.records() {
        if !crate::verified::position_transition(last_position, record.global_position()) {
            let kind = if record.global_position() <= last_position {
                ProjectionErrorKind::RecordOrder
            } else {
                ProjectionErrorKind::PositionGap
            };
            return Err(journal_error(kind, "global positions are not contiguous from one"));
        }
        validate_aggregate_record(&mut aggregates, record)?;
        let frame = decode_frame(record.frame_bytes(), CodecLimits::PRODUCTION).map_err(|_| {
            journal_error(ProjectionErrorKind::InvalidFrame, "record frame is not canonical B3")
        })?;
        let family = frame.header().family();
        let schema_version = frame.header().schema_version();
        validate_family(family, schema_version)?;
        projection.fold(&mut state, FoldContext { record, family, schema_version })?;
        last_position = record.global_position();
    }
    projection.finish(&mut state, export)?;
    state.validate()?;
    let payload = state.encode();
    let invariant_digest = state.invariant_digest();
    let checkpoint = Checkpoint::new(
        projection.schema().clone(),
        last_position,
        export.report().journal_head_digest(),
        &payload,
    );
    Ok(ReplayOutput {
        state,
        payload,
        checkpoint,
        invariant_digest,
        record_count: export.report().event_count(),
    })
}

fn validate_export_range(export: &IntegrityExport) -> Result<(), ProjectionError> {
    let count = u64::try_from(export.records().len())
        .map_err(|_| journal_error(ProjectionErrorKind::PositionGap, "record count exceeds u64"))?;
    if export.report().event_count() != count || export.report().last_position() != count {
        return Err(journal_error(
            ProjectionErrorKind::PositionGap,
            "integrity export range metadata does not match records",
        ));
    }
    Ok(())
}

fn validate_aggregate_record(
    cursors: &mut BTreeMap<AggregateKey, AggregateCursor>,
    record: &peritus_journal::CommittedRecord,
) -> Result<(), ProjectionError> {
    let prior = cursors.get(&record.aggregate()).copied();
    let (last_sequence, expected_id, expected_hash, expected_revision) = prior.map_or_else(
        || (0, None, Sha256Digest::new([0; 32]), None),
        |cursor| (cursor.sequence, Some(cursor.event_id), cursor.event_hash, Some(cursor.revision)),
    );
    if !crate::verified::sequence_transition(last_sequence, record.sequence().get())
        || record.previous_event_id() != expected_id
        || record.previous_event_hash() != expected_hash
    {
        return Err(journal_error(
            ProjectionErrorKind::AggregateOrder,
            "aggregate sequence or predecessor is invalid",
        ));
    }
    if expected_revision.is_some_and(|revision| revision != record.revision_digest()) {
        return Err(journal_error(
            ProjectionErrorKind::StaleRevision,
            "aggregate changed its exact revision binding during replay",
        ));
    }
    cursors.insert(
        record.aggregate(),
        AggregateCursor {
            sequence: record.sequence().get(),
            event_id: record.event_id(),
            event_hash: record.event_hash(),
            revision: record.revision_digest(),
        },
    );
    Ok(())
}

fn validate_family(family: u16, schema_version: u16) -> Result<(), ProjectionError> {
    let Some(registered) = FAMILIES.iter().find(|candidate| candidate.tag == family) else {
        return Err(journal_error(
            ProjectionErrorKind::UnsupportedFamily,
            format!("unknown frame family {family}"),
        ));
    };
    if schema_version != registered.schema_version {
        return Err(journal_error(
            ProjectionErrorKind::UnsupportedSchema,
            format!("family {family} schema {schema_version} is unsupported"),
        ));
    }
    Ok(())
}

fn journal_error(kind: ProjectionErrorKind, detail: impl Into<String>) -> ProjectionError {
    ProjectionError::new(kind, RecoveryClass::RepairJournal, "replay journal", detail)
}
