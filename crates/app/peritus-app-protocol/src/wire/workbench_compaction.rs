//! Canonical content-free deterministic compaction request and preview forms.

use super::primitive::{invalid, read_digest, read_id, write_digest, write_id};
use crate::{
    MAX_WORKBENCH_COMPACTION_ENTRIES, MAX_WORKBENCH_COMPACTION_FOCUS_BYTES,
    WorkbenchCompactionEntry, WorkbenchCompactionFocus, WorkbenchCompactionPreview,
    WorkbenchCompactionRequest, WorkbenchInvocationId,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};

pub(super) fn write_request(
    writer: &mut CanonicalWriter,
    value: &WorkbenchCompactionRequest,
) -> Result<(), CodecError> {
    super::workbench::write_query(writer, value.query())?;
    writer.write_u64(value.revision())?;
    writer.write_bool(value.focus().is_some())?;
    if let Some(focus) = value.focus() {
        writer.write_str(focus.as_str())?;
    }
    Ok(())
}

pub(super) fn read_request(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchCompactionRequest, CodecError> {
    let offset = reader.offset();
    let query = super::workbench::read_query(reader)?;
    let revision = reader.read_u64()?;
    let focus = if reader.read_bool()? {
        let text = reader.read_str()?;
        if text.len() > MAX_WORKBENCH_COMPACTION_FOCUS_BYTES {
            return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
        }
        Some(invalid(offset, WorkbenchCompactionFocus::new(text.to_owned()))?)
    } else {
        None
    };
    invalid(offset, WorkbenchCompactionRequest::new(query, revision, focus))
}

pub(super) fn write_preview(
    writer: &mut CanonicalWriter,
    value: &WorkbenchCompactionPreview,
) -> Result<(), CodecError> {
    write_request(writer, value.request())?;
    writer.write_u64(value.generation())?;
    writer.write_u32(value.recent_preserved())?;
    writer.write_u32(value.pinned_preserved())?;
    writer.write_u32(value.unresolved_preserved())?;
    writer.write_u32(value.unsavable_preserved())?;
    writer.write_u16(
        u16::try_from(value.entries().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, writer.len()))?,
    )?;
    for entry in value.entries() {
        write_id(writer, entry.invocation().as_bytes())?;
        write_digest(writer, entry.source_digest())?;
        writer.write_u64(entry.source_bytes())?;
        write_digest(writer, entry.replacement_digest())?;
        writer.write_u64(entry.replacement_bytes())?;
    }
    Ok(())
}

pub(super) fn read_preview(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchCompactionPreview, CodecError> {
    let offset = reader.offset();
    let request = read_request(reader)?;
    let generation = reader.read_u64()?;
    let recent_preserved = reader.read_u32()?;
    let pinned_preserved = reader.read_u32()?;
    let unresolved_preserved = reader.read_u32()?;
    let unsavable_preserved = reader.read_u32()?;
    let count = usize::from(reader.read_u16()?);
    if count > MAX_WORKBENCH_COMPACTION_ENTRIES {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        entries.push(invalid(
            offset,
            WorkbenchCompactionEntry::new(
                read_id(reader, WorkbenchInvocationId::new)?,
                read_digest(reader)?,
                reader.read_u64()?,
                read_digest(reader)?,
                reader.read_u64()?,
            ),
        )?);
    }
    invalid(
        offset,
        WorkbenchCompactionPreview::new(
            request,
            generation,
            recent_preserved,
            pinned_preserved,
            unresolved_preserved,
            unsavable_preserved,
            entries,
        ),
    )
}
