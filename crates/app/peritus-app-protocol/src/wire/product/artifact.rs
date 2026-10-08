//! Canonical referenced-run and product-artifact paging.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_run_settlement::CandidateStage;
use peritus_types::{RunId, WorkspaceId};

use crate::{
    ProductArtifactPage, ProductArtifactQuery, ProductArtifactReference,
    ProductDeliverableIndexEntry, ProductDeliverableIndexKind, ProductDeliverableIndexPage,
    ProductDeliverableIndexQuery, ProductDeliverableIndexReference, ProductDeliverableReference,
    ProductRunControlAction, ProductRunLegalControls, ProductRunOperationKind,
    ProductRunOperationReference, ProductRunOperationState, ProductRunPageCursor,
    ProductRunReferencePage, ProductRunReferencePageEntry, ProductRunReferenceQuery,
    ProductRunReferenceSnapshot, ProductRunStoreId,
};
use crate::wire::primitive::{invalid, read_digest, read_id, write_digest, write_id};

use super::{read_providers, read_run_page_cursor, write_providers, write_run_page_cursor};

pub(super) fn write_reference_query(
    writer: &mut CanonicalWriter,
    value: ProductRunReferenceQuery,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.run_id().is_some())?;
    if let Some(run) = value.run_id() {
        write_id(writer, run.as_bytes())?;
    }
    writer.write_option_tag(value.cursor().is_some())?;
    if let Some(cursor) = value.cursor() {
        write_run_page_cursor(writer, cursor)?;
    }
    Ok(())
}

pub(super) fn read_reference_query(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunReferenceQuery, CodecError> {
    let offset = reader.offset();
    let run = if reader.read_option_tag()? {
        Some(read_id(reader, RunId::new)?)
    } else {
        None
    };
    let cursor = if reader.read_option_tag()? {
        Some(read_run_page_cursor(reader)?)
    } else {
        None
    };
    match (run, cursor) {
        (Some(run), None) => Ok(ProductRunReferenceQuery::exact(run)),
        (None, Some(cursor)) => Ok(ProductRunReferenceQuery::after(cursor)),
        (None, None) => Ok(ProductRunReferenceQuery::first()),
        (Some(_), Some(_)) => Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset)),
    }
}

pub(super) fn write_artifact_query(
    writer: &mut CanonicalWriter,
    value: ProductArtifactQuery,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())?;
    write_artifact_reference(writer, value.source())?;
    writer.write_u64(value.offset())
}

pub(super) fn read_artifact_query(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductArtifactQuery, CodecError> {
    let offset = reader.offset();
    let run = read_id(reader, RunId::new)?;
    let source = read_artifact_reference(reader)?;
    let position = reader.read_u64()?;
    invalid(offset, ProductArtifactQuery::new(run, source, position))
}

pub(super) fn write_deliverable_index_query(
    writer: &mut CanonicalWriter,
    value: ProductDeliverableIndexQuery,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())?;
    writer.write_u16(value.kind().tag())?;
    write_index_reference(writer, value.index())?;
    writer.write_option_tag(value.after().is_some())?;
    if let Some(after) = value.after() {
        writer.write_u64(after)?;
    }
    Ok(())
}

pub(super) fn read_deliverable_index_query(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductDeliverableIndexQuery, CodecError> {
    let offset = reader.offset();
    let run = read_id(reader, RunId::new)?;
    let kind_offset = reader.offset();
    let kind = ProductDeliverableIndexKind::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, kind_offset))?;
    let index = read_index_reference(reader)?;
    let after = if reader.read_option_tag()? { Some(reader.read_u64()?) } else { None };
    invalid(offset, ProductDeliverableIndexQuery::new(run, kind, index, after))
}

pub(super) fn write_reference_page(
    writer: &mut CanonicalWriter,
    value: &ProductRunReferencePage,
) -> Result<(), CodecError> {
    write_id(writer, value.store().as_bytes())?;
    writer.write_collection_len(value.entries().len())?;
    for entry in value.entries() {
        writer.write_u64(entry.sequence())?;
        write_reference_snapshot(writer, entry.snapshot())?;
    }
    writer.write_option_tag(value.next().is_some())?;
    if let Some(next) = value.next() {
        write_run_page_cursor(writer, next)?;
    }
    Ok(())
}

pub(super) fn read_reference_page(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunReferencePage, CodecError> {
    let offset = reader.offset();
    let store = read_id(reader, ProductRunStoreId::new)?;
    let count = reader.read_collection_len(8 + 16 + 16 + 48)?;
    if count > crate::MAX_PRODUCT_RUN_PAGE {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut entries = reader.reserve_collection(count)?;
    for _ in 0..count {
        entries.push(invalid(
            offset,
            ProductRunReferencePageEntry::new(reader.read_u64()?, read_reference_snapshot(reader)?),
        )?);
    }
    let next = if reader.read_option_tag()? {
        Some(read_run_page_cursor(reader)?)
    } else {
        None
    };
    invalid(offset, ProductRunReferencePage::new(store, entries, next))
}

pub(super) fn write_artifact_page(
    writer: &mut CanonicalWriter,
    value: &ProductArtifactPage,
) -> Result<(), CodecError> {
    write_artifact_query(writer, value.query())?;
    writer.write_bytes(value.bytes())?;
    writer.write_option_tag(value.next().is_some())?;
    if let Some(next) = value.next() {
        writer.write_u64(next)?;
    }
    Ok(())
}

pub(super) fn read_artifact_page(
    reader: &mut CanonicalReader<'_>,
    maximum_chunk_bytes: usize,
) -> Result<ProductArtifactPage, CodecError> {
    let offset = reader.offset();
    let query = read_artifact_query(reader)?;
    let bytes = reader.read_bytes()?.to_vec();
    if bytes.len() > maximum_chunk_bytes {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let next = if reader.read_option_tag()? { Some(reader.read_u64()?) } else { None };
    invalid(offset, ProductArtifactPage::new(query, bytes, next))
}

pub(super) fn write_deliverable_index_page(
    writer: &mut CanonicalWriter,
    value: &ProductDeliverableIndexPage,
) -> Result<(), CodecError> {
    write_deliverable_index_query(writer, value.query())?;
    writer.write_collection_len(value.entries().len())?;
    for entry in value.entries() {
        writer.write_u64(entry.ordinal())?;
        write_artifact_reference(writer, entry.value())?;
    }
    writer.write_option_tag(value.next().is_some())?;
    if let Some(next) = value.next() {
        writer.write_u64(next)?;
    }
    Ok(())
}

pub(super) fn read_deliverable_index_page(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductDeliverableIndexPage, CodecError> {
    let offset = reader.offset();
    let query = read_deliverable_index_query(reader)?;
    let count = reader.read_collection_len(8 + 32 + 8)?;
    if count > crate::MAX_PRODUCT_DELIVERABLE_INDEX_PAGE {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut entries = reader.reserve_collection(count)?;
    for _ in 0..count {
        entries.push(ProductDeliverableIndexEntry::new(
            reader.read_u64()?,
            read_artifact_reference(reader)?,
        ));
    }
    let next = if reader.read_option_tag()? { Some(reader.read_u64()?) } else { None };
    invalid(offset, ProductDeliverableIndexPage::new(query, entries, next))
}

fn write_reference_snapshot(
    writer: &mut CanonicalWriter,
    value: &ProductRunReferenceSnapshot,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())?;
    write_id(writer, value.workspace_id().as_bytes())?;
    write_providers(writer, value.providers())?;
    writer.write_u16(value.phase().tag())?;
    writer.write_u32(value.cycle())?;
    write_artifact_reference(writer, value.task())?;
    write_artifact_reference(writer, value.status())?;
    for reference in [value.diff(), value.gates(), value.review(), value.summary()] {
        writer.write_option_tag(reference.is_some())?;
        if let Some(reference) = reference {
            write_artifact_reference(writer, reference)?;
        }
    }
    write_operation_reference(writer, value.operation())?;
    writer.write_option_tag(value.deliverable().is_some())?;
    if let Some(deliverable) = value.deliverable() {
        write_deliverable_reference(writer, deliverable)?;
    }
    writer.write_option_tag(value.settlement().is_some())?;
    if let Some(settlement) = value.settlement() {
        super::settlement::write_settlement(writer, &settlement)?;
    }
    Ok(())
}

fn read_reference_snapshot(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunReferenceSnapshot, CodecError> {
    let offset = reader.offset();
    let run = read_id(reader, RunId::new)?;
    let workspace = read_id(reader, WorkspaceId::new)?;
    let providers = read_providers(reader)?;
    let phase_offset = reader.offset();
    let phase = crate::ProductRunPhase::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, phase_offset))?;
    let cycle = reader.read_u32()?;
    let task = read_artifact_reference(reader)?;
    let status = read_artifact_reference(reader)?;
    let diff = read_optional_artifact_reference(reader)?;
    let gates = read_optional_artifact_reference(reader)?;
    let review = read_optional_artifact_reference(reader)?;
    let summary = read_optional_artifact_reference(reader)?;
    let operation = read_operation_reference(reader)?;
    let deliverable = if reader.read_option_tag()? {
        Some(read_deliverable_reference(reader)?)
    } else {
        None
    };
    let settlement = if reader.read_option_tag()? {
        Some(super::settlement::read_settlement(reader)?)
    } else {
        None
    };
    invalid(
        offset,
        ProductRunReferenceSnapshot::new(
            run,
            workspace,
            providers,
            phase,
            cycle,
            task,
            status,
            diff,
            gates,
            review,
            summary,
            operation,
            deliverable,
            settlement,
        ),
    )
}

fn write_operation_reference(
    writer: &mut CanonicalWriter,
    value: &ProductRunOperationReference,
) -> Result<(), CodecError> {
    writer.write_u16(value.kind().tag())?;
    writer.write_u16(value.state().tag())?;
    write_artifact_reference(writer, value.identity())?;
    write_artifact_reference(writer, value.known())?;
    writer.write_option_tag(value.uncertainty().is_some())?;
    if let Some(reference) = value.uncertainty() {
        write_artifact_reference(writer, reference)?;
    }
    let controls = value.legal_controls();
    for allowed in [
        controls.cancel(),
        controls.retry(),
        controls.accept(),
        controls.commit(),
        controls.export(),
        controls.discard(),
        controls.acknowledge(),
    ] {
        writer.write_bool(allowed)?;
    }
    Ok(())
}

fn read_operation_reference(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunOperationReference, CodecError> {
    let offset = reader.offset();
    let kind = ProductRunOperationKind::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))?;
    let state_offset = reader.offset();
    let state = ProductRunOperationState::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, state_offset))?;
    let identity = read_artifact_reference(reader)?;
    let known = read_artifact_reference(reader)?;
    let uncertainty = read_optional_artifact_reference(reader)?;
    let mut controls = ProductRunLegalControls::none();
    for action in [
        ProductRunControlAction::Cancel,
        ProductRunControlAction::Retry,
        ProductRunControlAction::Accept,
        ProductRunControlAction::Commit,
        ProductRunControlAction::Export,
        ProductRunControlAction::Discard,
        ProductRunControlAction::Acknowledge,
    ] {
        if reader.read_bool()? {
            controls = controls.with(action);
        }
    }
    invalid(
        offset,
        ProductRunOperationReference::new(kind, state, identity, known, uncertainty, controls),
    )
}

fn write_artifact_reference(
    writer: &mut CanonicalWriter,
    value: ProductArtifactReference,
) -> Result<(), CodecError> {
    write_digest(writer, value.digest())?;
    writer.write_u64(value.bytes())
}

fn read_artifact_reference(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductArtifactReference, CodecError> {
    let offset = reader.offset();
    let digest = read_digest(reader)?;
    let bytes = reader.read_u64()?;
    invalid(offset, ProductArtifactReference::new(digest, bytes))
}

fn read_optional_artifact_reference(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<ProductArtifactReference>, CodecError> {
    if reader.read_option_tag()? {
        read_artifact_reference(reader).map(Some)
    } else {
        Ok(None)
    }
}

fn write_index_reference(
    writer: &mut CanonicalWriter,
    value: ProductDeliverableIndexReference,
) -> Result<(), CodecError> {
    write_digest(writer, value.root())?;
    writer.write_u64(value.count())
}

fn read_index_reference(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductDeliverableIndexReference, CodecError> {
    Ok(ProductDeliverableIndexReference::new(read_digest(reader)?, reader.read_u64()?))
}

fn write_deliverable_reference(
    writer: &mut CanonicalWriter,
    value: &ProductDeliverableReference,
) -> Result<(), CodecError> {
    write_artifact_reference(writer, value.workspace_path())?;
    write_index_reference(writer, value.changed_paths())?;
    write_index_reference(writer, value.successful_commands())?;
    write_artifact_reference(writer, value.run_instructions())?;
    writer.write_u16(value.qualification().tag())?;
    writer.write_bool(value.accepted())?;
    for reference in [value.commit_revision(), value.export_path()] {
        writer.write_option_tag(reference.is_some())?;
        if let Some(reference) = reference {
            write_artifact_reference(writer, reference)?;
        }
    }
    writer.write_bool(value.discarded())
}

fn read_deliverable_reference(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductDeliverableReference, CodecError> {
    let offset = reader.offset();
    let workspace = read_artifact_reference(reader)?;
    let paths = read_index_reference(reader)?;
    let commands = read_index_reference(reader)?;
    let instructions = read_artifact_reference(reader)?;
    let stage_offset = reader.offset();
    let qualification = CandidateStage::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, stage_offset))?;
    let accepted = reader.read_bool()?;
    let commit = if reader.read_option_tag()? { Some(read_artifact_reference(reader)?) } else { None };
    let export = if reader.read_option_tag()? { Some(read_artifact_reference(reader)?) } else { None };
    let discarded = reader.read_bool()?;
    invalid(
        offset,
        ProductDeliverableReference::new(
            workspace,
            paths,
            commands,
            instructions,
            qualification,
            accepted,
            commit,
            export,
            discarded,
        ),
    )
}
