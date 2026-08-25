//! D1 and D2 quality observation codecs.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError};

use peritus_review::DispositionKind;

use crate::child::gates::GateObservationWire;
use crate::{
    GateChildObservation, HandoffId, OrchestratorLimits, ReviewChildObservation,
    ReviewFixerObservation, ReviewFixerRecord,
};

pub fn write_gate(
    writer: &mut CanonicalWriter,
    value: &GateChildObservation,
) -> Result<(), CodecError> {
    super::super::write_id(writer, value.run_id().as_bytes())?;
    super::super::write_id(writer, value.gate_run_id().as_bytes())?;
    super::super::write_revision(writer, value.revision())?;
    super::super::write_digest(writer, value.plan_digest())?;
    super::super::write_digest(writer, value.snapshot_digest())?;
    super::super::write_digest(writer, value.evidence_digest())?;
    writer.write_u8(super::super::gate_class_tag(value.class()))?;
    super::write_head(writer, value.head())
}

pub fn read_gate(reader: &mut CanonicalReader<'_>) -> Result<GateChildObservation, CodecError> {
    GateChildObservation::from_wire(&GateObservationWire {
        orchestrator_run_id: super::super::read_run_id(reader)?,
        gate_run_id: super::super::read_run_id(reader)?,
        revision: super::super::read_revision(reader)?,
        plan_digest: super::super::read_digest(reader)?,
        snapshot_digest: super::super::read_digest(reader)?,
        evidence_digest: super::super::read_digest(reader)?,
        class: super::super::read_gate_class(reader)?,
        head: super::read_head(reader)?,
    })
    .map_err(|_| super::super::invalid(reader))
}

pub fn write_review(
    writer: &mut CanonicalWriter,
    value: &ReviewChildObservation,
) -> Result<(), CodecError> {
    super::super::write_id(writer, value.run_id().as_bytes())?;
    super::super::write_revision(writer, value.revision())?;
    super::super::write_digest(writer, value.binding_digest())?;
    writer.write_bool(value.quorum_complete())?;
    writer.write_collection_len(value.unconserved_findings().len())?;
    for finding in value.unconserved_findings() {
        super::super::write_id(writer, finding.as_bytes())?;
    }
    writer.write_u8(super::super::review_class_tag(value.class()))?;
    super::write_head(writer, value.head())
}

pub fn read_review(reader: &mut CanonicalReader<'_>) -> Result<ReviewChildObservation, CodecError> {
    let run = super::super::read_run_id(reader)?;
    let revision = super::super::read_revision(reader)?;
    let binding = super::super::read_digest(reader)?;
    let quorum = reader.read_bool()?;
    let count =
        super::bounded_count(reader, usize::from(OrchestratorLimits::MAX_ARTIFACT_REFERENCES))?;
    let mut findings = Vec::with_capacity(count);
    for _ in 0..count {
        findings.push(super::super::read_finding_id(reader)?);
    }
    ReviewChildObservation::from_wire(
        run,
        revision,
        binding,
        quorum,
        findings,
        super::super::read_review_class(reader)?,
        super::read_head(reader)?,
    )
    .map_err(|_| super::super::invalid(reader))
}

pub(super) fn write_review_fixer(
    writer: &mut CanonicalWriter,
    value: &ReviewFixerObservation,
) -> Result<(), CodecError> {
    super::super::write_id(writer, value.handoff_id().as_bytes())?;
    super::super::write_id(writer, value.run_id().as_bytes())?;
    super::super::write_revision(writer, value.revision())?;
    super::super::write_digest(writer, value.binding_digest())?;
    writer.write_collection_len(value.records().len())?;
    for record in value.records() {
        super::super::write_id(writer, record.finding_id().as_bytes())?;
        writer.write_u8(disposition_tag(record.kind()))?;
        super::super::write_id(writer, record.actor().as_bytes())?;
        super::super::write_digest(writer, record.response_digest())?;
    }
    super::write_head(writer, value.head())
}

pub(super) fn read_review_fixer(
    reader: &mut CanonicalReader<'_>,
) -> Result<ReviewFixerObservation, CodecError> {
    let handoff = nominal(reader, HandoffId::new)?;
    let run = super::super::read_run_id(reader)?;
    let revision = super::super::read_revision(reader)?;
    let binding = super::super::read_digest(reader)?;
    let count =
        super::bounded_count(reader, usize::from(OrchestratorLimits::MAX_ARTIFACT_REFERENCES))?;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        records.push(ReviewFixerRecord::from_wire(
            super::super::read_finding_id(reader)?,
            read_disposition(reader)?,
            super::super::read_actor_id(reader)?,
            super::super::read_digest(reader)?,
        ));
    }
    ReviewFixerObservation::from_wire(
        handoff,
        run,
        revision,
        binding,
        records,
        super::read_head(reader)?,
    )
    .map_err(|_| super::super::invalid(reader))
}

const fn disposition_tag(value: DispositionKind) -> u8 {
    match value {
        DispositionKind::Fixed => 1,
        DispositionKind::Disputed => 2,
        DispositionKind::SupersessionProposed => 3,
        DispositionKind::WaiverRequested => 4,
        DispositionKind::Open
        | DispositionKind::ResolutionConfirmed
        | DispositionKind::InvalidationConfirmed
        | DispositionKind::Superseded
        | DispositionKind::Waived => 0,
    }
}

fn read_disposition(reader: &mut CanonicalReader<'_>) -> Result<DispositionKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(DispositionKind::Fixed),
        2 => Ok(DispositionKind::Disputed),
        3 => Ok(DispositionKind::SupersessionProposed),
        4 => Ok(DispositionKind::WaiverRequested),
        _ => Err(super::super::unknown(offset)),
    }
}

fn nominal<T, E>(
    reader: &mut CanonicalReader<'_>,
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    constructor(reader.read_fixed()?).map_err(|_| super::super::invalid_at(offset))
}
