//! Checked replay from journal genesis or an integrity-bound durable frontier.

use crate::encoding::{Decoder, decode_error, put_digest, put_key, put_u64};
use crate::{
    Checkpoint, FoldContext, Projection, ProjectionError, ProjectionErrorKind, ProjectionSchema,
    ProjectionState, RecoveryClass,
};
use peritus_codec::{CodecLimits, decode_frame, sha256};
use peritus_journal::{
    AggregateHead, AggregateKey, CommittedRecord, IntegrityExport, IntegrityReport,
};
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
    frontier_payload: Vec<u8>,
    frontier_digest: Sha256Digest,
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

    /// Borrows the canonical aggregate frontier needed for verified suffix replay.
    #[must_use]
    pub fn frontier_payload(&self) -> &[u8] {
        &self.frontier_payload
    }

    /// Returns the digest binding the canonical aggregate frontier.
    #[must_use]
    pub const fn frontier_digest(&self) -> Sha256Digest {
        self.frontier_digest
    }

    /// Returns the number of records consumed.
    #[must_use]
    pub const fn record_count(&self) -> u64 {
        self.record_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AggregateCursor {
    sequence: u64,
    event_id: EventId,
    event_hash: Sha256Digest,
    revision: Sha256Digest,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ReplayFrontier {
    aggregates: BTreeMap<AggregateKey, AggregateCursor>,
}

impl ReplayFrontier {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut bytes = b"peritus-projection-frontier-v1\0".to_vec();
        put_u64(&mut bytes, self.aggregates.len() as u64);
        for (key, cursor) in &self.aggregates {
            put_key(&mut bytes, *key);
            put_u64(&mut bytes, cursor.sequence);
            bytes.extend_from_slice(cursor.event_id.as_bytes());
            put_digest(&mut bytes, cursor.event_hash);
            put_digest(&mut bytes, cursor.revision);
        }
        bytes
    }

    pub(crate) fn decode(payload: &[u8]) -> Result<Self, ProjectionError> {
        let mut decoder = Decoder::new(payload, b"peritus-projection-frontier-v1\0")?;
        let count = decoder.count(106)?;
        let mut aggregates = BTreeMap::new();
        for _ in 0..count {
            let key = decoder.key()?;
            let cursor = AggregateCursor {
                sequence: decoder.u64()?,
                event_id: EventId::new(decoder.event_id_bytes()?)
                    .map_err(|_| decode_error("projection frontier event identity is invalid"))?,
                event_hash: decoder.digest()?,
                revision: decoder.digest()?,
            };
            if cursor.sequence == 0 {
                return Err(decode_error("projection frontier sequence is zero"));
            }
            aggregates.insert(key, cursor);
        }
        decoder.finish()?;
        let frontier = Self { aggregates };
        if frontier.encode() != payload {
            return Err(decode_error("projection frontier is not canonical"));
        }
        Ok(frontier)
    }

    fn apply(&mut self, record: &CommittedRecord) -> Result<(), ProjectionError> {
        let prior = self.aggregates.get(&record.aggregate()).copied();
        let (last_sequence, expected_id, expected_hash, expected_revision) = prior.map_or_else(
            || (0, None, Sha256Digest::new([0; 32]), None),
            |cursor| {
                (
                    cursor.sequence,
                    Some(cursor.event_id),
                    cursor.event_hash,
                    Some(cursor.revision),
                )
            },
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
        self.aggregates.insert(
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

    fn matches_heads(&self, heads: &[AggregateHead]) -> bool {
        self.aggregates.len() == heads.len()
            && self.aggregates.iter().zip(heads).all(|((key, cursor), head)| {
                *key == head.key()
                    && cursor.sequence == head.sequence().get()
                    && cursor.event_id == head.event_id()
                    && cursor.event_hash == head.event_hash()
            })
    }
}

pub(crate) struct ReplayCheckpoint<S> {
    state: S,
    frontier: ReplayFrontier,
    last_position: u64,
    record_count: u64,
}

impl<S: ProjectionState> ReplayCheckpoint<S> {
    pub(crate) fn genesis(state: S) -> Self {
        Self { state, frontier: ReplayFrontier::default(), last_position: 0, record_count: 0 }
    }

    pub(crate) fn restore(
        payload: &[u8],
        invariant_digest: Sha256Digest,
        frontier_payload: &[u8],
        last_position: u64,
        record_count: u64,
    ) -> Result<Self, ProjectionError> {
        let state = S::decode(payload)?;
        state.validate()?;
        if state.encode() != payload || state.invariant_digest() != invariant_digest {
            return Err(decode_error("projection state checkpoint digest is invalid"));
        }
        if record_count != last_position {
            return Err(decode_error("projection state checkpoint count is invalid"));
        }
        let frontier = ReplayFrontier::decode(frontier_payload)?;
        Ok(Self { state, frontier, last_position, record_count })
    }

    pub(crate) const fn last_position(&self) -> u64 {
        self.last_position
    }

    pub(crate) const fn record_count(&self) -> u64 {
        self.record_count
    }

    pub(crate) const fn state(&self) -> &S {
        &self.state
    }

    pub(crate) fn frontier_payload(&self) -> Vec<u8> {
        self.frontier.encode()
    }

    pub(crate) fn apply<P: Projection<State = S>>(
        &mut self,
        projection: &P,
        record: &CommittedRecord,
    ) -> Result<(), ProjectionError> {
        if !crate::verified::position_transition(self.last_position, record.global_position()) {
            let kind = if record.global_position() <= self.last_position {
                ProjectionErrorKind::RecordOrder
            } else {
                ProjectionErrorKind::PositionGap
            };
            return Err(journal_error(kind, "global positions are not contiguous"));
        }
        self.frontier.apply(record)?;
        let frame = decode_frame(record.frame_bytes(), CodecLimits::PRODUCTION).map_err(|_| {
            journal_error(ProjectionErrorKind::InvalidFrame, "record frame is not canonical B3")
        })?;
        let family = frame.header().family();
        let schema_version = frame.header().schema_version();
        validate_family(family, schema_version)?;
        projection.fold(&mut self.state, FoldContext { record, family, schema_version })?;
        self.last_position = record.global_position();
        self.record_count = self
            .record_count
            .checked_add(1)
            .ok_or_else(|| journal_error(ProjectionErrorKind::PositionGap, "record count overflows"))?;
        Ok(())
    }

    pub(crate) fn apply_supplement<P: Projection<State = S>>(
        &mut self,
        projection: &P,
        export: &IntegrityExport,
    ) -> Result<(), ProjectionError> {
        projection.fold_supplement(&mut self.state, export)
    }

    pub(crate) fn finish(
        self,
        schema: &ProjectionSchema,
        report: &IntegrityReport,
        heads: &[AggregateHead],
    ) -> Result<ReplayOutput<S>, ProjectionError> {
        if report.last_position() != self.last_position
            || report.event_count() != self.record_count
            || !self.frontier.matches_heads(heads)
        {
            return Err(journal_error(
                ProjectionErrorKind::StaleCheckpoint,
                "accepted replay frontier does not match the checked journal head",
            ));
        }
        self.state.validate()?;
        let payload = self.state.encode();
        let invariant_digest = self.state.invariant_digest();
        let frontier_payload = self.frontier.encode();
        let frontier_digest = sha256(&frontier_payload);
        let checkpoint = Checkpoint::new(
            schema.clone(),
            self.last_position,
            report.journal_head_digest(),
            &payload,
        );
        Ok(ReplayOutput {
            state: self.state,
            payload,
            checkpoint,
            invariant_digest,
            frontier_payload,
            frontier_digest,
            record_count: self.record_count,
        })
    }
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
    let mut replay = ReplayCheckpoint::genesis(projection.genesis());
    for record in export.records() {
        replay.apply(projection, record)?;
    }
    projection.finish(&mut replay.state, export)?;
    replay.finish(projection.schema(), export.report(), export.heads())
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

fn validate_family(family: u16, schema_version: u16) -> Result<(), ProjectionError> {
    let Some(registered) = FAMILIES.iter().find(|candidate| candidate.tag == family) else {
        return Err(journal_error(
            ProjectionErrorKind::UnsupportedFamily,
            format!("unknown frame family {family}"),
        ));
    };
    if !registered.supports(schema_version) {
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

#[cfg(test)]
mod tests {
    use super::validate_family;
    use crate::ProjectionErrorKind;

    #[test]
    fn scheduler_family_validation_accepts_v1_v2_and_rejects_v0_v3() {
        assert!(validate_family(71, 1).is_ok());
        assert!(validate_family(71, 2).is_ok());
        for version in [0, 3] {
            let error = validate_family(71, version).expect_err("unsupported scheduler schema");
            assert_eq!(error.kind(), ProjectionErrorKind::UnsupportedSchema);
        }
        let error = validate_family(3, 2).expect_err("non-scheduler family remains v1 only");
        assert_eq!(error.kind(), ProjectionErrorKind::UnsupportedSchema);
    }
}
