//! Candidate settlement encoding layered over the legacy product snapshot bytes.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceRecord, EvidenceStatus,
    QualificationEvidence, RunDisposition, RunSettlement, SettlementCause, SettlementReducer,
};
use peritus_types::{RunId, WorkspaceId};

use crate::{MAX_PRODUCT_RUNS, ProductRunSettlementSnapshot};

use super::{read_snapshot_inner, write_snapshot_inner};
use crate::wire::primitive::{invalid, read_digest, read_id, write_digest, write_id};

pub(in crate::wire) fn write_settlement_snapshot(
    writer: &mut CanonicalWriter,
    value: &ProductRunSettlementSnapshot,
) -> Result<(), CodecError> {
    write_snapshot_inner(writer, value.snapshot(), true)?;
    write_settlement(writer, value.settlement())
}

pub(in crate::wire) fn read_settlement_snapshot(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunSettlementSnapshot, CodecError> {
    let offset = reader.offset();
    let mut snapshot = read_snapshot_inner(reader, true)?;
    let settlement = read_settlement(reader)?;
    if let Some(checkpoint) = settlement.checkpoint() {
        let deliverable = snapshot
            .deliverable()
            .cloned()
            .ok_or_else(|| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))?
            .restore_qualification(checkpoint.stage());
        snapshot = snapshot.with_deliverable(deliverable);
    }
    invalid(offset, ProductRunSettlementSnapshot::new(snapshot, settlement))
}

pub(in crate::wire) fn write_settlement_snapshots(
    writer: &mut CanonicalWriter,
    values: &[ProductRunSettlementSnapshot],
) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        write_settlement_snapshot(writer, value)?;
    }
    Ok(())
}

pub(in crate::wire) fn read_settlement_snapshots(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<ProductRunSettlementSnapshot>, CodecError> {
    let offset = reader.offset();
    let length = reader.read_collection_len()?;
    if length > MAX_PRODUCT_RUNS {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    (0..length).map(|_| read_settlement_snapshot(reader)).collect()
}

fn write_settlement(writer: &mut CanonicalWriter, value: &RunSettlement) -> Result<(), CodecError> {
    writer.write_u16(value.disposition().tag())?;
    writer.write_u16(value.cause().tag())?;
    writer.write_option_tag(value.checkpoint().is_some())?;
    if let Some(checkpoint) = value.checkpoint() {
        write_checkpoint(writer, checkpoint)?;
    }
    Ok(())
}

fn read_settlement(reader: &mut CanonicalReader<'_>) -> Result<RunSettlement, CodecError> {
    let offset = reader.offset();
    let disposition_offset = reader.offset();
    let disposition = RunDisposition::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, disposition_offset))?;
    let cause_offset = reader.offset();
    let cause = SettlementCause::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, cause_offset))?;
    let checkpoint = if reader.read_option_tag()? { Some(read_checkpoint(reader)?) } else { None };
    let mut reducer = SettlementReducer::new();
    if let Some(checkpoint) = checkpoint {
        invalid(offset, reducer.observe(checkpoint))?;
    }
    let settlement = invalid(offset, reducer.settle(cause))?;
    if settlement.disposition() != disposition {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    Ok(settlement)
}

fn write_checkpoint(
    writer: &mut CanonicalWriter,
    value: &CandidateCheckpoint,
) -> Result<(), CodecError> {
    write_candidate_identity(writer, value.identity())?;
    writer.write_u16(value.stage().tag())?;
    write_evidence(writer, value.gates())?;
    write_evidence(writer, value.obligations())?;
    write_evidence(writer, value.review())
}

fn read_checkpoint(reader: &mut CanonicalReader<'_>) -> Result<CandidateCheckpoint, CodecError> {
    let offset = reader.offset();
    let identity = read_candidate_identity(reader)?;
    let stage_offset = reader.offset();
    let stage = CandidateStage::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, stage_offset))?;
    let gates = read_evidence(reader)?;
    let obligations = read_evidence(reader)?;
    let review = read_evidence(reader)?;
    invalid(offset, CandidateCheckpoint::new(identity, stage, gates, obligations, review))
}

fn write_candidate_identity(
    writer: &mut CanonicalWriter,
    value: &CandidateIdentity,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())?;
    write_id(writer, value.workspace_id().as_bytes())?;
    write_digest(writer, value.candidate_digest())?;
    writer.write_u64(value.conversation_revision())?;
    writer.write_u64(value.checkpoint_sequence())
}

fn read_candidate_identity(
    reader: &mut CanonicalReader<'_>,
) -> Result<CandidateIdentity, CodecError> {
    let offset = reader.offset();
    invalid(
        offset,
        CandidateIdentity::new(
            read_id(reader, RunId::new)?,
            read_id(reader, WorkspaceId::new)?,
            read_digest(reader)?,
            reader.read_u64()?,
            reader.read_u64()?,
        ),
    )
}

fn write_evidence(
    writer: &mut CanonicalWriter,
    value: &EvidenceStatus<QualificationEvidence>,
) -> Result<(), CodecError> {
    writer.write_u16(value.tag())?;
    if let Some(record) = value.record() {
        write_candidate_identity(writer, record.provenance())?;
        writer.write_u16(record.value().tag())?;
    }
    Ok(())
}

fn read_evidence(
    reader: &mut CanonicalReader<'_>,
) -> Result<EvidenceStatus<QualificationEvidence>, CodecError> {
    let offset = reader.offset();
    let tag = reader.read_u16()?;
    if tag == 1 {
        return Ok(EvidenceStatus::Missing);
    }
    if !matches!(tag, 2..=4) {
        return Err(CodecError::at(CodecErrorKind::UnknownTag, offset));
    }
    let provenance = read_candidate_identity(reader)?;
    let evidence_offset = reader.offset();
    let value = QualificationEvidence::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, evidence_offset))?;
    let record = EvidenceRecord::new(provenance, value);
    match tag {
        2 => Ok(EvidenceStatus::Current(record)),
        3 => Ok(EvidenceStatus::Failed(record)),
        4 => Ok(EvidenceStatus::Stale(record)),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_codec::CodecLimits;

    #[test]
    fn unknown_evidence_status_rejects_before_reading_a_record() {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_u16(u16::MAX).expect("write tag");
        let bytes = writer.into_bytes();
        let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);

        assert_eq!(
            read_evidence(&mut reader).expect_err("unknown evidence status").kind(),
            CodecErrorKind::UnknownTag,
        );
        assert_eq!(reader.offset(), 2);
    }
}
