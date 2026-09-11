//! Canonical effective-permission projection codec.

use super::workbench;
use crate::{
    WorkbenchPermissionCapability, WorkbenchPermissionChange, WorkbenchPermissionEntry,
    WorkbenchPermissionProvenance, WorkbenchPermissions, WorkbenchWorkspaceTrust,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};

pub(super) fn write_permissions(
    writer: &mut CanonicalWriter,
    value: &WorkbenchPermissions,
) -> Result<(), CodecError> {
    workbench::write_query(writer, value.query())?;
    writer.write_u64(value.conversation_revision())?;
    writer.write_u64(value.authority_revision())?;
    writer.write_u16(value.trust().tag())?;
    for entry in value.entries() {
        writer.write_u16(entry.capability().tag())?;
        writer.write_bool(entry.host_allowed())?;
        writer.write_bool(entry.effective_allowed())?;
        writer.write_u16(entry.provenance().tag())?;
        writer.write_bool(entry.explicit_approval_still_required())?;
    }
    Ok(())
}

pub(super) fn read_permissions(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchPermissions, CodecError> {
    let offset = reader.offset();
    let query = workbench::read_query(reader)?;
    let conversation_revision = reader.read_u64()?;
    let authority_revision = reader.read_u64()?;
    let trust_offset = reader.offset();
    let trust = WorkbenchWorkspaceTrust::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, trust_offset))?;
    let mut entries = Vec::with_capacity(4);
    for _ in 0..4 {
        let tag_offset = reader.offset();
        let capability = WorkbenchPermissionCapability::from_tag(reader.read_u16()?)
            .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, tag_offset))?;
        let host_allowed = reader.read_bool()?;
        let effective_allowed = reader.read_bool()?;
        let provenance_offset = reader.offset();
        let provenance = WorkbenchPermissionProvenance::from_tag(reader.read_u16()?)
            .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, provenance_offset))?;
        let explicit_approval = reader.read_bool()?;
        entries.push(
            WorkbenchPermissionEntry::new(
                capability,
                host_allowed,
                effective_allowed,
                provenance,
                explicit_approval,
            )
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))?,
        );
    }
    let entries: [WorkbenchPermissionEntry; 4] = entries
        .try_into()
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))?;
    WorkbenchPermissions::new(query, conversation_revision, authority_revision, trust, entries)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub(super) fn write_change(
    writer: &mut CanonicalWriter,
    value: WorkbenchPermissionChange,
) -> Result<(), CodecError> {
    writer.write_u64(value.expected_authority_revision())?;
    writer.write_u16(value.capability().tag())?;
    writer.write_bool(value.allowed())
}

pub(super) fn read_change(
    reader: &mut CanonicalReader<'_>,
) -> Result<WorkbenchPermissionChange, CodecError> {
    let revision = reader.read_u64()?;
    let offset = reader.offset();
    let capability = WorkbenchPermissionCapability::from_tag(reader.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))?;
    Ok(WorkbenchPermissionChange::new(revision, capability, reader.read_bool()?))
}
