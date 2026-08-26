//! Exact C7/C0 and ordinary-artifact provenance checks.

use crate::{DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery};
use peritus_codec::{CodecLimits, decode_frame};
use peritus_journal::{AggregateKind, CommittedRecord, IntegrityExport};
use peritus_trace::{
    ProjectedObservation, TRACE_OBSERVATION_FAMILY, TRACE_OBSERVATION_SCHEMA, trace_schema_digest,
};
use peritus_types::EventId;
use std::collections::BTreeMap;

pub(super) fn record_index(
    export: &IntegrityExport,
) -> Result<BTreeMap<EventId, &CommittedRecord>, DebuggerError> {
    let mut records = BTreeMap::new();
    for record in export.records() {
        if records.insert(record.event_id(), record).is_some() {
            return Err(corruption("C0 integrity export repeats an event identity"));
        }
    }
    Ok(records)
}

pub(super) fn validate_projection_row(
    projected: &ProjectedObservation,
    record: &CommittedRecord,
) -> Result<(), DebuggerError> {
    let frame = decode_frame(record.frame_bytes(), CodecLimits::PRODUCTION)
        .map_err(|_| corruption("C0 trace frame is not canonical B3 data"))?;
    let observation = projected.observation();
    let record_matches = [
        record.global_position() == projected.journal_position(),
        record.event_id() == observation.event_id(),
        record.aggregate().kind() == AggregateKind::Trace,
        record.aggregate().id().as_bytes() == observation.trace_id().as_bytes(),
        record.revision_digest() == trace_schema_digest(),
        record.frame_digest() == projected.frame_digest(),
        record.frame_bytes() == projected.frame_bytes(),
        record.causal_parents() == observation.causal_events(),
    ]
    .into_iter()
    .all(|matches| matches);
    let frame_matches = [
        frame.header().family() == TRACE_OBSERVATION_FAMILY,
        frame.header().schema_version() == TRACE_OBSERVATION_SCHEMA,
    ]
    .into_iter()
    .all(|matches| matches);
    let exact = record_matches && frame_matches;
    if exact {
        Ok(())
    } else {
        Err(corruption("C7 projection entry differs from its exact C0 record"))
    }
}

pub(super) fn artifact_is_committed(
    export: &IntegrityExport,
    artifact: &super::SelectedArtifact,
    positions: &BTreeMap<EventId, u64>,
) -> bool {
    export.artifact_references().iter().any(|reference| {
        reference.artifact_digest() == artifact.digest().sha256()
            && artifact.source_event().is_none_or(|event| {
                positions.get(&event).is_some_and(|position| {
                    reference.first_position() <= *position
                        && *position <= reference.last_position()
                })
            })
    })
}

pub(super) fn corruption(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Corruption,
        DebuggerOperation::SelectEvidence,
        DebuggerRecovery::Quarantine,
        detail,
    )
}
