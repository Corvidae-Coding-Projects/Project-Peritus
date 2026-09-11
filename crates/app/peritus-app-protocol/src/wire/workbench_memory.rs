//! Canonical project-guidance queries, mutations, active records and content-free tombstones.

use super::primitive::{invalid, read_id, unknown, write_id};
use crate::{
    MAX_WORKBENCH_GUIDANCE_BYTES, MAX_WORKBENCH_GUIDANCE_PAGE, MAX_WORKBENCH_GUIDANCE_REASON_BYTES,
    WorkbenchGuidanceContent, WorkbenchGuidanceForget, WorkbenchGuidanceIdentity,
    WorkbenchGuidancePin, WorkbenchGuidancePrior, WorkbenchGuidanceReason, WorkbenchGuidanceRecord,
    WorkbenchGuidanceRevision, WorkbenchGuidanceSave, WorkbenchGuidanceScope,
    WorkbenchGuidanceScopeChange, WorkbenchGuidanceSelection, WorkbenchGuidanceSource,
    WorkbenchGuidanceText, WorkbenchGuidanceTombstone, WorkbenchGuidanceValidation,
    WorkbenchGuidanceVersion, WorkbenchMemory, WorkbenchMemoryQuery, WorkbenchMemoryRow,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::{Sha256Digest, WorkspaceId};

#[cfg(test)]
mod tests;

pub(super) fn write_query(
    writer: &mut CanonicalWriter,
    query: WorkbenchMemoryQuery,
) -> Result<(), CodecError> {
    super::workbench::write_query(writer, query.query())?;
    writer.write_u64(query.dependency_revision())?;
    writer.write_u32(query.offset())?;
    writer.write_bool(query.include_forgotten())
}

pub(super) fn read_query(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchMemoryQuery, CodecError> {
    let offset = reader.offset();
    invalid(
        offset,
        WorkbenchMemoryQuery::new(
            super::workbench::read_query(reader)?,
            reader.read_u64()?,
            reader.read_u32()?,
            reader.read_bool()?,
        ),
    )
}

pub(super) fn write_memory(
    writer: &mut CanonicalWriter,
    memory: &WorkbenchMemory,
) -> Result<(), CodecError> {
    write_query(writer, memory.query())?;
    writer.write_u64(memory.dependency_revision())?;
    writer.write_u32(memory.total())?;
    writer.write_u16(
        u16::try_from(memory.rows().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, writer.len()))?,
    )?;
    for row in memory.rows() {
        match row {
            WorkbenchMemoryRow::Active(record) => {
                writer.write_u16(1)?;
                write_record(writer, record)?;
            }
            WorkbenchMemoryRow::Forgotten(tombstone) => {
                writer.write_u16(2)?;
                write_tombstone(writer, tombstone)?;
            }
        }
    }
    Ok(())
}

pub(super) fn read_memory(reader: &mut CanonicalReader<'_>) -> Result<WorkbenchMemory, CodecError> {
    let offset = reader.offset();
    let query = read_query(reader)?;
    let dependency_revision = reader.read_u64()?;
    let total = reader.read_u32()?;
    let count = usize::from(reader.read_u16()?);
    if count > MAX_WORKBENCH_GUIDANCE_PAGE {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        rows.push(match reader.read_u16()? {
            1 => WorkbenchMemoryRow::Active(read_record(reader)?),
            2 => WorkbenchMemoryRow::Forgotten(read_tombstone(reader)?),
            _ => return unknown(offset),
        });
    }
    invalid(offset, WorkbenchMemory::new(query, dependency_revision, total, rows))
}

pub(super) fn write_save(
    writer: &mut CanonicalWriter,
    value: &WorkbenchGuidanceSave,
) -> Result<(), CodecError> {
    writer.write_u64(value.expected_dependency_revision())?;
    write_content(writer, value.content())?;
    writer.write_bool(value.pinned())
}

pub(super) fn read_save(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchGuidanceSave, CodecError> {
    Ok(WorkbenchGuidanceSave::new(reader.read_u64()?, read_content(reader)?, reader.read_bool()?))
}

pub(super) fn write_revision(
    writer: &mut CanonicalWriter,
    value: &WorkbenchGuidanceRevision,
) -> Result<(), CodecError> {
    write_selection(writer, value.selection())?;
    writer.write_u64(value.expected_dependency_revision())?;
    write_content(writer, value.content())
}

pub(super) fn read_revision(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchGuidanceRevision, CodecError> {
    Ok(WorkbenchGuidanceRevision::new(
        read_selection(reader)?,
        reader.read_u64()?,
        read_content(reader)?,
    ))
}

pub(super) fn write_pin(
    writer: &mut CanonicalWriter,
    value: WorkbenchGuidancePin,
) -> Result<(), CodecError> {
    write_selection(writer, value.selection())?;
    writer.write_u64(value.expected_dependency_revision())?;
    writer.write_bool(value.pinned())
}

pub(super) fn read_pin(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchGuidancePin, CodecError> {
    Ok(WorkbenchGuidancePin::new(read_selection(reader)?, reader.read_u64()?, reader.read_bool()?))
}

pub(super) fn write_scope_change(
    writer: &mut CanonicalWriter,
    value: WorkbenchGuidanceScopeChange,
) -> Result<(), CodecError> {
    write_selection(writer, value.selection())?;
    writer.write_u64(value.expected_dependency_revision())?;
    write_scope(writer, value.scope())
}

pub(super) fn read_scope_change(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchGuidanceScopeChange, CodecError> {
    Ok(WorkbenchGuidanceScopeChange::new(
        read_selection(reader)?,
        reader.read_u64()?,
        read_scope(reader)?,
    ))
}

pub(super) fn write_forget(
    writer: &mut CanonicalWriter,
    value: &WorkbenchGuidanceForget,
) -> Result<(), CodecError> {
    write_selection(writer, value.selection())?;
    writer.write_u64(value.expected_dependency_revision())?;
    writer.write_str(value.reason().as_str())
}

pub(super) fn read_forget(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchGuidanceForget, CodecError> {
    let selection = read_selection(reader)?;
    let dependency_revision = reader.read_u64()?;
    let reason = read_reason(reader)?;
    Ok(WorkbenchGuidanceForget::new(selection, dependency_revision, reason))
}

fn write_record(
    writer: &mut CanonicalWriter,
    record: &WorkbenchGuidanceRecord,
) -> Result<(), CodecError> {
    write_identity(writer, record.identity())?;
    writer.write_u64(record.version().record())?;
    writer.write_u64(record.version().dependency())?;
    write_content(writer, record.content())?;
    writer.write_bool(record.pinned())?;
    write_id(writer, record.last_validation().operation().as_bytes())?;
    writer.write_fixed(record.last_validation().content_digest().as_bytes())
}

fn read_record(reader: &mut CanonicalReader<'_>) -> Result<WorkbenchGuidanceRecord, CodecError> {
    let offset = reader.offset();
    let identity = read_identity(reader)?;
    let version =
        invalid(offset, WorkbenchGuidanceVersion::new(reader.read_u64()?, reader.read_u64()?))?;
    let content = read_content(reader)?;
    let pinned = reader.read_bool()?;
    let validation = WorkbenchGuidanceValidation::new(
        read_id(reader, crate::ControlOperationId::new)?,
        Sha256Digest::new(reader.read_fixed()?),
    );
    invalid(offset, WorkbenchGuidanceRecord::new(identity, version, content, pinned, validation))
}

fn write_tombstone(
    writer: &mut CanonicalWriter,
    tombstone: &WorkbenchGuidanceTombstone,
) -> Result<(), CodecError> {
    write_identity(writer, tombstone.identity())?;
    writer.write_u64(tombstone.prior().revision())?;
    writer.write_fixed(tombstone.prior().digest().as_bytes())?;
    write_scope(writer, tombstone.prior().scope())?;
    writer.write_bool(tombstone.prior().pinned())?;
    write_id(writer, tombstone.forgotten_by().as_bytes())?;
    writer.write_str(tombstone.reason().as_str())?;
    writer.write_u64(tombstone.dependency_revision())
}

fn read_tombstone(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchGuidanceTombstone, CodecError> {
    let offset = reader.offset();
    let identity = read_identity(reader)?;
    let prior = invalid(
        offset,
        WorkbenchGuidancePrior::new(
            reader.read_u64()?,
            Sha256Digest::new(reader.read_fixed()?),
            read_scope(reader)?,
            reader.read_bool()?,
        ),
    )?;
    let forgotten_by = read_id(reader, crate::ControlOperationId::new)?;
    let reason = read_reason(reader)?;
    let dependency_revision = reader.read_u64()?;
    invalid(
        offset,
        WorkbenchGuidanceTombstone::new(identity, prior, forgotten_by, reason, dependency_revision),
    )
}

fn write_identity(
    writer: &mut CanonicalWriter,
    identity: WorkbenchGuidanceIdentity,
) -> Result<(), CodecError> {
    write_id(writer, identity.id().as_bytes())?;
    write_id(writer, identity.workspace().as_bytes())
}

fn read_identity(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchGuidanceIdentity, CodecError> {
    Ok(WorkbenchGuidanceIdentity::new(
        read_id(reader, crate::ControlOperationId::new)?,
        read_id(reader, WorkspaceId::new)?,
    ))
}

fn write_selection(
    writer: &mut CanonicalWriter,
    selection: WorkbenchGuidanceSelection,
) -> Result<(), CodecError> {
    write_id(writer, selection.id().as_bytes())?;
    writer.write_u64(selection.expected_revision())
}

fn read_selection(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchGuidanceSelection, CodecError> {
    let offset = reader.offset();
    let id = read_id(reader, crate::ControlOperationId::new)?;
    invalid(offset, WorkbenchGuidanceSelection::new(id, reader.read_u64()?))
}

fn write_content(
    writer: &mut CanonicalWriter,
    content: &WorkbenchGuidanceContent,
) -> Result<(), CodecError> {
    writer.write_str(content.text().as_str())?;
    write_source(writer, content.source())?;
    write_scope(writer, content.scope())
}

fn read_content(reader: &mut CanonicalReader<'_>) -> Result<WorkbenchGuidanceContent, CodecError> {
    let offset = reader.offset();
    let text = read_text(reader)?;
    let source = read_source(reader)?;
    let scope = read_scope(reader)?;
    invalid(offset, WorkbenchGuidanceContent::new(text, source, scope))
}

fn write_source(
    writer: &mut CanonicalWriter,
    source: WorkbenchGuidanceSource,
) -> Result<(), CodecError> {
    match source {
        WorkbenchGuidanceSource::UserAuthored => writer.write_u16(1),
        WorkbenchGuidanceSource::AcceptedPublicReply { operation, invocation, digest } => {
            writer.write_u16(2)?;
            write_id(writer, operation.as_bytes())?;
            write_id(writer, invocation.as_bytes())?;
            writer.write_fixed(digest.as_bytes())
        }
    }
}

fn read_source(reader: &mut CanonicalReader<'_>) -> Result<WorkbenchGuidanceSource, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(WorkbenchGuidanceSource::UserAuthored),
        2 => Ok(WorkbenchGuidanceSource::AcceptedPublicReply {
            operation: read_id(reader, crate::ControlOperationId::new)?,
            invocation: read_id(reader, crate::WorkbenchInvocationId::new)?,
            digest: Sha256Digest::new(reader.read_fixed()?),
        }),
        _ => unknown(offset),
    }
}

fn write_scope(
    writer: &mut CanonicalWriter,
    scope: WorkbenchGuidanceScope,
) -> Result<(), CodecError> {
    match scope {
        WorkbenchGuidanceScope::Project => writer.write_u16(1),
        WorkbenchGuidanceScope::Conversation(conversation) => {
            writer.write_u16(2)?;
            write_id(writer, conversation.as_bytes())
        }
    }
}

fn read_scope(reader: &mut CanonicalReader<'_>) -> Result<WorkbenchGuidanceScope, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(WorkbenchGuidanceScope::Project),
        2 => Ok(WorkbenchGuidanceScope::Conversation(read_id(reader, crate::ConversationId::new)?)),
        _ => unknown(offset),
    }
}

fn read_text(reader: &mut CanonicalReader<'_>) -> Result<WorkbenchGuidanceText, CodecError> {
    let offset = reader.offset();
    let text = reader.read_str()?;
    if text.len() > MAX_WORKBENCH_GUIDANCE_BYTES {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    invalid(offset, WorkbenchGuidanceText::new(text.to_owned()))
}

fn read_reason(reader: &mut CanonicalReader<'_>) -> Result<WorkbenchGuidanceReason, CodecError> {
    let offset = reader.offset();
    let reason = reader.read_str()?;
    if reason.len() > MAX_WORKBENCH_GUIDANCE_REASON_BYTES {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    invalid(offset, WorkbenchGuidanceReason::new(reason.to_owned()))
}
