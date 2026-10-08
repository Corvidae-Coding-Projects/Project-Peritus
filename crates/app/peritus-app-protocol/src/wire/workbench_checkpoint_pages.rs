//! Canonical page and continuation forms for complete checkpoint coverage.

use super::{
    primitive::{invalid, read_digest, read_id, unknown, write_digest, write_id},
    workbench_checkpoints,
};
use crate::{
    ControlOperationId, WorkbenchCheckpointCoveragePage, WorkbenchCheckpointName,
    WorkbenchCheckpointPageRequest, WorkbenchCheckpointReceipt, WorkbenchCoverageCursor,
    WorkbenchCoverageSection, WorkbenchRestoreStatus, WorkbenchRestoreSummary,
    WorkbenchRewindConfirmation, WorkbenchRewindCoveragePage, WorkbenchRewindPageRequest,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};

pub(super) fn write_restore_summary(
    writer: &mut CanonicalWriter,
    value: &WorkbenchRestoreSummary,
) -> Result<(), CodecError> {
    write_id(writer, value.restore().as_bytes())?;
    write_id(writer, value.checkpoint().as_bytes())?;
    write_id(writer, value.recovery_checkpoint().as_bytes())?;
    super::workbench::write_query(writer, value.query())?;
    writer.write_u64(value.accepted_revision())?;
    writer.write_u16(match value.status() {
        WorkbenchRestoreStatus::Applied => 1,
        WorkbenchRestoreStatus::Conflict => 2,
        WorkbenchRestoreStatus::RecoveryRequired => 3,
    })?;
    writer.write_u64(value.restored_paths())?;
    writer.write_u64(value.conflicting_paths())?;
    write_digest(writer, value.fingerprint())
}

pub(super) fn read_restore_summary(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchRestoreSummary, CodecError> {
    let offset = reader.offset();
    let restore = read_id(reader, ControlOperationId::new)?;
    let checkpoint = read_id(reader, ControlOperationId::new)?;
    let recovery = read_id(reader, ControlOperationId::new)?;
    let query = super::workbench::read_query(reader)?;
    let revision = reader.read_u64()?;
    let status = match reader.read_u16()? {
        1 => WorkbenchRestoreStatus::Applied,
        2 => WorkbenchRestoreStatus::Conflict,
        3 => WorkbenchRestoreStatus::RecoveryRequired,
        _ => return unknown(offset),
    };
    let restored_paths = reader.read_u64()?;
    let conflicting_paths = reader.read_u64()?;
    let fingerprint = read_digest(reader)?;
    invalid(
        offset,
        WorkbenchRestoreSummary::new(
            restore,
            checkpoint,
            recovery,
            query,
            revision,
            status,
            restored_paths,
            conflicting_paths,
            fingerprint,
        ),
    )
}

pub(super) fn write_checkpoint_request(
    writer: &mut CanonicalWriter,
    value: WorkbenchCheckpointPageRequest,
) -> Result<(), CodecError> {
    super::workbench::write_query(writer, value.query())?;
    writer.write_u64(value.revision())?;
    write_id(writer, value.checkpoint().as_bytes())?;
    write_cursor_option(writer, value.cursor())
}

pub(super) fn read_checkpoint_request(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchCheckpointPageRequest, CodecError> {
    let query = super::workbench::read_query(reader)?;
    let offset = reader.offset();
    let revision = reader.read_u64()?;
    if revision == 0 {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    let checkpoint = read_id(reader, ControlOperationId::new)?;
    invalid(
        offset,
        WorkbenchCheckpointPageRequest::new(
            query,
            revision,
            checkpoint,
            read_cursor_option(reader)?,
        ),
    )
}

pub(super) fn write_rewind_request(
    writer: &mut CanonicalWriter,
    value: WorkbenchRewindPageRequest,
) -> Result<(), CodecError> {
    workbench_checkpoints::write_request(writer, value.request())?;
    write_cursor_option(writer, value.cursor())
}

pub(super) fn read_rewind_request(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchRewindPageRequest, CodecError> {
    Ok(WorkbenchRewindPageRequest::new(
        workbench_checkpoints::read_request(reader)?,
        read_cursor_option(reader)?,
    ))
}

pub(super) fn write_checkpoint_page(
    writer: &mut CanonicalWriter,
    page: &WorkbenchCheckpointCoveragePage,
) -> Result<(), CodecError> {
    write_id(writer, page.checkpoint().as_bytes())?;
    super::workbench::write_query(writer, page.query())?;
    writer.write_u64(page.selected_revision())?;
    writer.write_u64(page.accepted_revision())?;
    writer.write_str(page.name().as_str())?;
    workbench_checkpoints::write_references(writer, page.references())?;
    write_digest(writer, page.fingerprint())?;
    writer.write_u64(page.total_paths())?;
    writer.write_u64(page.total_exclusions())?;
    writer.write_u64(page.total_external_effects())?;
    write_section(writer, page.section())?;
    writer.write_u64(page.offset())?;
    write_paths(writer, page.paths())?;
    write_strings(writer, page.exclusions())?;
    write_strings(writer, page.external_effects())?;
    write_cursor_option(writer, page.next())
}

pub(super) fn read_checkpoint_page(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchCheckpointCoveragePage, CodecError> {
    let offset = reader.offset();
    let checkpoint = read_id(reader, ControlOperationId::new)?;
    let query = super::workbench::read_query(reader)?;
    let selected_revision = reader.read_u64()?;
    let accepted_revision = reader.read_u64()?;
    let name = invalid(offset, WorkbenchCheckpointName::new(reader.read_str()?.to_owned()))?;
    let references = workbench_checkpoints::read_references(reader)?;
    let fingerprint = read_digest(reader)?;
    let total_paths = reader.read_u64()?;
    let total_exclusions = reader.read_u64()?;
    let total_external_effects = reader.read_u64()?;
    let section = read_section(reader)?;
    let page_offset = reader.read_u64()?;
    let paths = read_paths(reader)?;
    let exclusions = read_strings(reader, 4_610)?;
    let effects = read_strings(reader, 512)?;
    let next = read_cursor_option(reader)?;
    let receipt = invalid(
        offset,
        WorkbenchCheckpointReceipt::new(
            checkpoint,
            query,
            accepted_revision,
            name,
            references,
            paths.clone(),
            exclusions.clone(),
            effects.clone(),
        ),
    )?;
    invalid(
        offset,
        WorkbenchCheckpointCoveragePage::new(
            &receipt,
            selected_revision,
            fingerprint,
            total_paths,
            total_exclusions,
            total_external_effects,
            section,
            page_offset,
            paths,
            exclusions,
            effects,
            next,
        ),
    )
}

pub(super) fn write_rewind_page(
    writer: &mut CanonicalWriter,
    page: &WorkbenchRewindCoveragePage,
) -> Result<(), CodecError> {
    workbench_checkpoints::write_request(writer, page.confirmation().request())?;
    write_digest(writer, page.confirmation().preview_digest())?;
    writer.write_u64(page.total_paths())?;
    writer.write_u64(page.total_exclusions())?;
    writer.write_u64(page.total_external_effects())?;
    write_section(writer, page.section())?;
    writer.write_u64(page.offset())?;
    write_rewind_paths(writer, page.paths())?;
    write_strings(writer, page.exclusions())?;
    write_strings(writer, page.external_effects())?;
    write_cursor_option(writer, page.next())
}

pub(super) fn read_rewind_page(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchRewindCoveragePage, CodecError> {
    let request = workbench_checkpoints::read_request(reader)?;
    let confirmation = WorkbenchRewindConfirmation::new(request, read_digest(reader)?);
    let total_paths = reader.read_u64()?;
    let total_exclusions = reader.read_u64()?;
    let total_external_effects = reader.read_u64()?;
    let section = read_section(reader)?;
    let page_offset = reader.read_u64()?;
    let paths = read_rewind_paths(reader)?;
    let exclusions = read_strings(reader, 4_610)?;
    let effects = read_strings(reader, 512)?;
    let next = read_cursor_option(reader)?;
    invalid(
        reader.offset(),
        WorkbenchRewindCoveragePage::new(
            confirmation,
            total_paths,
            total_exclusions,
            total_external_effects,
            section,
            page_offset,
            paths,
            exclusions,
            effects,
            next,
        ),
    )
}

fn write_paths(
    writer: &mut CanonicalWriter,
    values: &[crate::WorkbenchCheckpointPath],
) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        workbench_checkpoints::write_checkpoint_path(writer, value)?;
    }
    Ok(())
}

fn write_rewind_paths(
    writer: &mut CanonicalWriter,
    values: &[crate::WorkbenchRewindPath],
) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        workbench_checkpoints::write_rewind_path(writer, value)?;
    }
    Ok(())
}

fn read_rewind_paths(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<crate::WorkbenchRewindPath>, CodecError> {
    let count = reader.read_collection_len()?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(workbench_checkpoints::read_rewind_path(reader)?);
    }
    Ok(values)
}

fn read_paths(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<crate::WorkbenchCheckpointPath>, CodecError> {
    let count = reader.read_collection_len()?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(workbench_checkpoints::read_checkpoint_path(reader)?);
    }
    Ok(values)
}

fn write_strings(writer: &mut CanonicalWriter, values: &[String]) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        writer.write_str(value)?;
    }
    Ok(())
}

fn read_strings(
    reader: &mut CanonicalReader<'_>,
    maximum: usize,
) -> Result<Vec<String>, CodecError> {
    let offset = reader.offset();
    let count = reader.read_collection_len()?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let value = reader.read_str()?;
        if value.len() > maximum {
            return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
        }
        values.push(value.to_owned());
    }
    Ok(values)
}

fn write_cursor_option(
    writer: &mut CanonicalWriter,
    cursor: Option<WorkbenchCoverageCursor>,
) -> Result<(), CodecError> {
    writer.write_bool(cursor.is_some())?;
    if let Some(cursor) = cursor {
        write_cursor(writer, cursor)?;
    }
    Ok(())
}

fn read_cursor_option(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<WorkbenchCoverageCursor>, CodecError> {
    if reader.read_bool()? { read_cursor(reader).map(Some) } else { Ok(None) }
}

fn write_cursor(
    writer: &mut CanonicalWriter,
    cursor: WorkbenchCoverageCursor,
) -> Result<(), CodecError> {
    write_section(writer, cursor.section())?;
    writer.write_u64(cursor.offset())?;
    write_digest(writer, cursor.fingerprint())
}

fn read_cursor(reader: &mut CanonicalReader<'_>) -> Result<WorkbenchCoverageCursor, CodecError> {
    Ok(WorkbenchCoverageCursor::new(
        read_section(reader)?,
        reader.read_u64()?,
        read_digest(reader)?,
    ))
}

fn write_section(
    writer: &mut CanonicalWriter,
    section: WorkbenchCoverageSection,
) -> Result<(), CodecError> {
    writer.write_u16(match section {
        WorkbenchCoverageSection::Paths => 1,
        WorkbenchCoverageSection::Exclusions => 2,
        WorkbenchCoverageSection::ExternalEffects => 3,
    })
}

fn read_section(reader: &mut CanonicalReader<'_>) -> Result<WorkbenchCoverageSection, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(WorkbenchCoverageSection::Paths),
        2 => Ok(WorkbenchCoverageSection::Exclusions),
        3 => Ok(WorkbenchCoverageSection::ExternalEffects),
        _ => unknown(offset),
    }
}
