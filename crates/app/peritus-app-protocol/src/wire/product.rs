//! Canonical product-run request and observation encoding.

pub(super) mod observations;
mod artifact;
mod settlement;

pub(super) use artifact::{
    read_artifact_page, read_artifact_query, read_deliverable_index_page,
    read_deliverable_index_query, read_reference_page, read_reference_query,
    write_artifact_page, write_artifact_query, write_deliverable_index_page,
    write_deliverable_index_query, write_reference_page, write_reference_query,
};
pub(super) use settlement::{read_settlement_snapshot, write_settlement_snapshot};

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_run_settlement::CandidateStage;
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

use crate::{
    ProductDeliverable, ProductInteractionQuery, ProductProviderSelection, ProductRunControl,
    ProductRunControlAction, ProductRunLegalControls, ProductRunOperation, ProductRunOperationKind,
    ProductRunOperationState, ProductRunPage, ProductRunPageCursor, ProductRunPageEntry,
    ProductRunPageQuery, ProductRunPhase, ProductRunQuery, ProductRunSnapshot, ProductRunStoreId,
};

use super::primitive::{invalid, read_id, write_id};

pub(super) fn write_run_control(
    writer: &mut CanonicalWriter,
    value: ProductRunControl,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())?;
    writer.write_u16(value.action().tag())
}

pub(super) fn read_run_control(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunControl, CodecError> {
    let run_id = read_id(reader, RunId::new)?;
    let offset = reader.offset();
    let action = ProductRunControlAction::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))?;
    Ok(ProductRunControl::new(run_id, action))
}

pub(super) fn write_run_query(
    writer: &mut CanonicalWriter,
    value: ProductRunQuery,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.run_id().is_some())?;
    if let Some(run_id) = value.run_id() {
        write_id(writer, run_id.as_bytes())?;
    } else {
        writer.write_u64(value.offset())?;
    }
    Ok(())
}

pub(super) fn read_run_query(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunQuery, CodecError> {
    if reader.read_option_tag()? {
        Ok(ProductRunQuery::exact(read_id(reader, RunId::new)?))
    } else {
        Ok(ProductRunQuery::page(reader.read_u64()?))
    }
}

pub(super) fn write_run_page_query(
    writer: &mut CanonicalWriter,
    value: ProductRunPageQuery,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.cursor().is_some())?;
    if let Some(cursor) = value.cursor() {
        write_run_page_cursor(writer, cursor)?;
    }
    Ok(())
}

pub(super) fn read_run_page_query(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunPageQuery, CodecError> {
    if reader.read_option_tag()? {
        Ok(ProductRunPageQuery::after(read_run_page_cursor(reader)?))
    } else {
        Ok(ProductRunPageQuery::first())
    }
}

pub(super) fn write_run_page(
    writer: &mut CanonicalWriter,
    value: &ProductRunPage,
) -> Result<(), CodecError> {
    write_id(writer, value.store().as_bytes())?;
    writer.write_collection_len(value.entries().len())?;
    for entry in value.entries() {
        writer.write_u64(entry.sequence())?;
        observations::write_observation(writer, entry.observation())?;
    }
    writer.write_option_tag(value.next().is_some())?;
    if let Some(cursor) = value.next() {
        write_run_page_cursor(writer, cursor)?;
    }
    Ok(())
}

pub(super) fn read_run_page(reader: &mut CanonicalReader<'_>) -> Result<ProductRunPage, CodecError> {
    let offset = reader.offset();
    let store = read_id(reader, ProductRunStoreId::new)?;
    let count = reader.read_collection_len(
        8 + 1 + 2 * 16 + 3 * 16 + 2 + 4 + 6 * 4 + 2 * 2 + 3 * 4 + 7 + 1,
    )?;
    if count > crate::MAX_PRODUCT_RUN_PAGE {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut entries = reader.reserve_collection(count)?;
    for _ in 0..count {
        entries.push(invalid(
            offset,
            ProductRunPageEntry::new(
                reader.read_u64()?,
                observations::read_observation(reader, offset)?,
            ),
        )?);
    }
    let next = if reader.read_option_tag()? {
        Some(read_run_page_cursor(reader)?)
    } else {
        None
    };
    invalid(offset, ProductRunPage::new(store, entries, next))
}

fn write_run_page_cursor(
    writer: &mut CanonicalWriter,
    value: ProductRunPageCursor,
) -> Result<(), CodecError> {
    write_id(writer, value.store().as_bytes())?;
    writer.write_u64(value.highwater_sequence())?;
    write_id(writer, value.highwater_run().as_bytes())?;
    writer.write_u64(value.after_sequence())?;
    write_id(writer, value.after_run().as_bytes())
}

fn read_run_page_cursor(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunPageCursor, CodecError> {
    let offset = reader.offset();
    let store = read_id(reader, ProductRunStoreId::new)?;
    let highwater_sequence = reader.read_u64()?;
    let highwater_run = read_id(reader, RunId::new)?;
    let after_sequence = reader.read_u64()?;
    let after_run = read_id(reader, RunId::new)?;
    invalid(
        offset,
        ProductRunPageCursor::new(
            store,
            highwater_sequence,
            highwater_run,
            after_sequence,
            after_run,
        ),
    )
}

pub(super) fn write_conversation_query(
    writer: &mut CanonicalWriter,
    value: ProductInteractionQuery,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())
}

pub(super) fn read_conversation_query(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductInteractionQuery, CodecError> {
    Ok(ProductInteractionQuery::new(read_id(reader, RunId::new)?))
}

pub(super) fn write_snapshot(
    writer: &mut CanonicalWriter,
    value: &ProductRunSnapshot,
) -> Result<(), CodecError> {
    write_snapshot_inner(writer, value, false)
}

fn write_snapshot_inner(
    writer: &mut CanonicalWriter,
    value: &ProductRunSnapshot,
    allow_unqualified: bool,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())?;
    write_id(writer, value.workspace_id().as_bytes())?;
    write_providers(writer, value.providers())?;
    writer.write_u16(value.phase().tag())?;
    writer.write_u32(value.cycle())?;
    for text in
        [value.task(), value.status(), value.diff(), value.gates(), value.review(), value.summary()]
    {
        writer.write_str(text)?;
    }
    write_operation(writer, value.operation())?;
    writer.write_option_tag(value.deliverable().is_some())?;
    if let Some(deliverable) = value.deliverable() {
        write_deliverable(writer, deliverable, allow_unqualified)?;
    }
    Ok(())
}

pub(super) fn read_snapshot(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductRunSnapshot, CodecError> {
    read_snapshot_inner(reader, false)
}

fn read_snapshot_inner(
    reader: &mut CanonicalReader<'_>,
    allow_unqualified: bool,
) -> Result<ProductRunSnapshot, CodecError> {
    let offset = reader.offset();
    let run_id = read_id(reader, RunId::new)?;
    let workspace_id = read_id(reader, WorkspaceId::new)?;
    let providers = read_providers(reader)?;
    let phase_offset = reader.offset();
    let phase = ProductRunPhase::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, phase_offset))?;
    let cycle = reader.read_u32()?;
    let task = reader.read_str()?.to_owned();
    let status = reader.read_str()?.to_owned();
    let diff = reader.read_str()?.to_owned();
    let gates = reader.read_str()?.to_owned();
    let review = reader.read_str()?.to_owned();
    let summary = reader.read_str()?.to_owned();
    let operation = read_operation(reader)?;
    let snapshot = invalid(
        offset,
        ProductRunSnapshot::new(
            run_id,
            workspace_id,
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
        ),
    )?;
    if reader.read_option_tag()? {
        Ok(snapshot.with_deliverable(read_deliverable(reader, allow_unqualified)?))
    } else {
        Ok(snapshot)
    }
}

fn write_operation(
    writer: &mut CanonicalWriter,
    value: &ProductRunOperation,
) -> Result<(), CodecError> {
    writer.write_u16(value.kind().tag())?;
    writer.write_u16(value.state().tag())?;
    writer.write_str(value.identity())?;
    writer.write_str(value.known())?;
    writer.write_str(value.uncertainty())?;
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

fn read_operation(reader: &mut CanonicalReader<'_>) -> Result<ProductRunOperation, CodecError> {
    let offset = reader.offset();
    let kind = ProductRunOperationKind::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))?;
    let state_offset = reader.offset();
    let state = ProductRunOperationState::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, state_offset))?;
    let identity = reader.read_str()?.to_owned();
    let known = reader.read_str()?.to_owned();
    let uncertainty = reader.read_str()?.to_owned();
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
    invalid(offset, ProductRunOperation::new(kind, state, identity, known, uncertainty, controls))
}

fn write_deliverable(
    writer: &mut CanonicalWriter,
    value: &ProductDeliverable,
    allow_unqualified: bool,
) -> Result<(), CodecError> {
    if !allow_unqualified && value.qualification() != CandidateStage::Qualified {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, writer.len()));
    }
    writer.write_str(value.workspace_path())?;
    writer.write_collection_len(value.changed_paths().len())?;
    for path in value.changed_paths() {
        writer.write_str(path)?;
    }
    writer.write_collection_len(value.successful_commands().len())?;
    for command in value.successful_commands() {
        writer.write_str(command)?;
    }
    writer.write_str(value.run_instructions())?;
    writer.write_bool(value.accepted())?;
    writer.write_str(value.commit_revision())?;
    writer.write_str(value.export_path())?;
    writer.write_bool(value.discarded())
}

fn read_deliverable(
    reader: &mut CanonicalReader<'_>,
    allow_unqualified: bool,
) -> Result<ProductDeliverable, CodecError> {
    let offset = reader.offset();
    let workspace_path = reader.read_str()?.to_owned();
    let path_count = reader.read_collection_len(4)?;
    let mut changed_paths = reader.reserve_collection(path_count)?;
    for _ in 0..path_count {
        changed_paths.push(reader.read_str()?.to_owned());
    }
    let command_count = reader.read_collection_len(4)?;
    let mut successful_commands = reader.reserve_collection(command_count)?;
    for _ in 0..command_count {
        successful_commands.push(reader.read_str()?.to_owned());
    }
    let run_instructions = reader.read_str()?.to_owned();
    let accepted = reader.read_bool()?;
    let commit_revision = reader.read_str()?.to_owned();
    let export_path = reader.read_str()?.to_owned();
    let discarded = reader.read_bool()?;
    let deliverable = if allow_unqualified {
        ProductDeliverable::restore_candidate(
            workspace_path,
            changed_paths,
            successful_commands,
            run_instructions,
            accepted,
            commit_revision,
            export_path,
            discarded,
        )
    } else {
        ProductDeliverable::restore(
            workspace_path,
            changed_paths,
            successful_commands,
            run_instructions,
            accepted,
            commit_revision,
            export_path,
            discarded,
        )
    };
    invalid(offset, deliverable)
}

pub(super) fn write_providers(
    writer: &mut CanonicalWriter,
    value: ProductProviderSelection,
) -> Result<(), CodecError> {
    for profile in [value.writer(), value.reviewer(), value.fixer()] {
        write_id(writer, profile.as_bytes())?;
    }
    Ok(())
}

pub(super) fn read_providers(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProductProviderSelection, CodecError> {
    Ok(ProductProviderSelection::new(
        read_id(reader, ProviderProfileId::new)?,
        read_id(reader, ProviderProfileId::new)?,
        read_id(reader, ProviderProfileId::new)?,
    ))
}
