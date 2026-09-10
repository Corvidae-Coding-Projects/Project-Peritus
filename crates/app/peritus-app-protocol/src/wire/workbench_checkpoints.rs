//! Canonical bounded checkpoint and rewind forms.

use super::primitive::{invalid, read_digest, read_id, unknown, write_digest, write_id};
use crate::{
    ControlOperationId, MAX_WORKBENCH_CHECKPOINT_PATHS, WorkbenchCheckpointFileMode as FileMode,
    WorkbenchCheckpointName, WorkbenchCheckpointPath, WorkbenchCheckpointReceipt,
    WorkbenchCheckpointReferences, WorkbenchCheckpointVersion as Version, WorkbenchRestoreReceipt,
    WorkbenchRestoreStatus, WorkbenchRewindDisposition, WorkbenchRewindMode, WorkbenchRewindPath,
    WorkbenchRewindPreview, WorkbenchRewindRequest,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind, CodecLimits};
use peritus_types::Sha256Digest;

#[cfg(test)]
mod tests;

pub(super) fn write_checkpoint_receipt(
    w: &mut CanonicalWriter,
    value: &WorkbenchCheckpointReceipt,
) -> Result<(), CodecError> {
    write_id(w, value.checkpoint().as_bytes())?;
    super::workbench::write_query(w, value.query())?;
    w.write_u64(value.accepted_revision())?;
    w.write_str(value.name().as_str())?;
    write_references(w, value.references())?;
    write_checkpoint_paths(w, value.paths())?;
    write_strings(w, value.exclusions())?;
    write_strings(w, value.external_effects())
}

pub(super) fn read_checkpoint_receipt(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchCheckpointReceipt, CodecError> {
    let offset = r.offset();
    let checkpoint = read_id(r, ControlOperationId::new)?;
    let query = super::workbench::read_query(r)?;
    let revision = r.read_u64()?;
    let name = invalid(offset, WorkbenchCheckpointName::new(r.read_str()?.to_owned()))?;
    let references = read_references(r)?;
    let paths = read_checkpoint_paths(r)?;
    let exclusions = read_strings(r, 512)?;
    let external = read_strings(r, 512)?;
    invalid(
        offset,
        WorkbenchCheckpointReceipt::new(
            checkpoint, query, revision, name, references, paths, exclusions, external,
        ),
    )
}

pub(super) fn write_request(
    w: &mut CanonicalWriter,
    value: WorkbenchRewindRequest,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    w.write_u64(value.revision())?;
    write_id(w, value.checkpoint().as_bytes())?;
    w.write_u16(match value.mode() {
        WorkbenchRewindMode::FilesOnly => 1,
        WorkbenchRewindMode::ConversationOnly => 2,
        WorkbenchRewindMode::Combined => 3,
    })?;
    if let Some(child) = value.child() {
        write_id(w, child.as_bytes())?;
        w.write_bool(value.allocation().is_some())?;
        if let Some(budget) = value.allocation() {
            super::workbench_library::write_budget(w, budget)?;
        }
    }
    Ok(())
}

pub(super) fn read_request(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchRewindRequest, CodecError> {
    let offset = r.offset();
    let request = invalid(
        offset,
        WorkbenchRewindRequest::new(
            super::workbench::read_query(r)?,
            r.read_u64()?,
            read_id(r, ControlOperationId::new)?,
        ),
    )?;
    let mode = match r.read_u16()? {
        1 => return Ok(request),
        2 => WorkbenchRewindMode::ConversationOnly,
        3 => WorkbenchRewindMode::Combined,
        _ => return unknown(offset),
    };
    let child = read_id(r, crate::ConversationId::new)?;
    let budget =
        if r.read_bool()? { Some(super::workbench_library::read_budget(r)?) } else { None };
    invalid(offset, request.with_branch(mode, child, budget))
}

pub(super) fn write_preview(
    w: &mut CanonicalWriter,
    value: &WorkbenchRewindPreview,
) -> Result<(), CodecError> {
    write_request(w, value.request())?;
    write_digest(w, value.preview_digest())?;
    write_rewind_paths(w, value.paths())?;
    write_strings(w, value.exclusions())?;
    write_strings(w, value.external_effects())?;
    w.write_bool(value.conversation_history_preserved())?;
    w.write_bool(value.accounting_preserved())
}

pub(super) fn read_preview(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchRewindPreview, CodecError> {
    let offset = r.offset();
    let request = read_request(r)?;
    let digest = read_digest(r)?;
    let paths = read_rewind_paths(r)?;
    let exclusions = read_strings(r, 512)?;
    let external = read_strings(r, 512)?;
    if !r.read_bool()? || !r.read_bool()? {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    let preview =
        invalid(offset, WorkbenchRewindPreview::new(request, paths, exclusions, external))?;
    if preview.preview_digest() != digest {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    Ok(preview)
}

pub(super) fn write_restore_receipt(
    w: &mut CanonicalWriter,
    value: &WorkbenchRestoreReceipt,
) -> Result<(), CodecError> {
    write_id(w, value.restore().as_bytes())?;
    write_id(w, value.checkpoint().as_bytes())?;
    write_id(w, value.recovery_checkpoint().as_bytes())?;
    super::workbench::write_query(w, value.query())?;
    w.write_u64(value.accepted_revision())?;
    w.write_u16(match value.status() {
        WorkbenchRestoreStatus::Applied => 1,
        WorkbenchRestoreStatus::Conflict => 2,
        WorkbenchRestoreStatus::RecoveryRequired => 3,
    })?;
    write_strings(w, value.restored())?;
    write_strings(w, value.conflicts())?;
    write_strings(w, value.external_effects())
}

pub(super) fn read_restore_receipt(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchRestoreReceipt, CodecError> {
    let offset = r.offset();
    let restore = read_id(r, ControlOperationId::new)?;
    let checkpoint = read_id(r, ControlOperationId::new)?;
    let recovery = read_id(r, ControlOperationId::new)?;
    let query = super::workbench::read_query(r)?;
    let revision = r.read_u64()?;
    let status = match r.read_u16()? {
        1 => WorkbenchRestoreStatus::Applied,
        2 => WorkbenchRestoreStatus::Conflict,
        3 => WorkbenchRestoreStatus::RecoveryRequired,
        _ => return unknown(offset),
    };
    invalid(
        offset,
        WorkbenchRestoreReceipt::new(
            restore,
            checkpoint,
            recovery,
            query,
            revision,
            status,
            read_strings(r, 4096)?,
            read_strings(r, 4096)?,
            read_strings(r, 4096)?,
        ),
    )
}

pub fn preview_fingerprint(
    request: WorkbenchRewindRequest,
    paths: &[WorkbenchRewindPath],
    exclusions: &[String],
    external: &[String],
) -> Result<Sha256Digest, CodecError> {
    let mut w = CanonicalWriter::new(CodecLimits::PRODUCTION);
    w.write_fixed(b"peritus-workbench-rewind-preview-v1")?;
    write_request(&mut w, request)?;
    write_rewind_paths(&mut w, paths)?;
    write_strings(&mut w, exclusions)?;
    write_strings(&mut w, external)?;
    Ok(peritus_codec::sha256(w.as_slice()))
}

fn write_references(
    w: &mut CanonicalWriter,
    value: WorkbenchCheckpointReferences,
) -> Result<(), CodecError> {
    w.write_u64(value.source_conversation_revision())?;
    w.write_u64(value.context_generation())?;
    w.write_u64(value.brief_revision())?;
    w.write_bool(value.goal_revision().is_some())?;
    if let Some(goal) = value.goal_revision() {
        w.write_u64(goal)?;
    }
    Ok(())
}

fn read_references(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchCheckpointReferences, CodecError> {
    let source = r.read_u64()?;
    let context = r.read_u64()?;
    let brief = r.read_u64()?;
    let goal = if r.read_bool()? { Some(r.read_u64()?) } else { None };
    Ok(WorkbenchCheckpointReferences::new(source, context, brief, goal))
}

fn write_checkpoint_paths(
    w: &mut CanonicalWriter,
    paths: &[WorkbenchCheckpointPath],
) -> Result<(), CodecError> {
    write_count(w, paths.len())?;
    for path in paths {
        w.write_str(path.path())?;
        write_version(w, path.checkpoint())?;
        w.write_bool(path.expected_current().is_some())?;
        if let Some(version) = path.expected_current() {
            write_version(w, version)?;
        }
    }
    Ok(())
}

fn read_checkpoint_paths(
    r: &mut CanonicalReader<'_>,
) -> Result<Vec<WorkbenchCheckpointPath>, CodecError> {
    let offset = r.offset();
    let count = read_count(r, offset)?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let path = r.read_str()?.to_owned();
        let checkpoint = read_version(r)?;
        let expected = if r.read_bool()? { Some(read_version(r)?) } else { None };
        values.push(invalid(offset, WorkbenchCheckpointPath::new(path, checkpoint, expected))?);
    }
    Ok(values)
}

fn write_rewind_paths(
    w: &mut CanonicalWriter,
    paths: &[WorkbenchRewindPath],
) -> Result<(), CodecError> {
    write_count(w, paths.len())?;
    for path in paths {
        w.write_str(path.path())?;
        write_version(w, path.checkpoint())?;
        w.write_bool(path.expected_current().is_some())?;
        if let Some(version) = path.expected_current() {
            write_version(w, version)?;
        }
        write_version(w, path.observed_current())?;
        w.write_u16(match path.disposition() {
            WorkbenchRewindDisposition::Restore => 1,
            WorkbenchRewindDisposition::Unchanged => 2,
            WorkbenchRewindDisposition::Conflict => 3,
            WorkbenchRewindDisposition::Unsealed => 4,
        })?;
    }
    Ok(())
}

fn read_rewind_paths(r: &mut CanonicalReader<'_>) -> Result<Vec<WorkbenchRewindPath>, CodecError> {
    let offset = r.offset();
    let count = read_count(r, offset)?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let path = r.read_str()?.to_owned();
        let checkpoint = read_version(r)?;
        let expected = if r.read_bool()? { Some(read_version(r)?) } else { None };
        let observed = read_version(r)?;
        let disposition = match r.read_u16()? {
            1 => WorkbenchRewindDisposition::Restore,
            2 => WorkbenchRewindDisposition::Unchanged,
            3 => WorkbenchRewindDisposition::Conflict,
            4 => WorkbenchRewindDisposition::Unsealed,
            _ => return unknown(offset),
        };
        values.push(invalid(
            offset,
            WorkbenchRewindPath::new(path, checkpoint, expected, observed, disposition),
        )?);
    }
    Ok(values)
}

fn write_version(w: &mut CanonicalWriter, value: Version) -> Result<(), CodecError> {
    match value {
        Version::Absent => w.write_u16(0),
        Version::Present { digest, bytes, mode } => {
            w.write_u16(1)?;
            write_digest(w, digest)?;
            w.write_u64(bytes)?;
            w.write_u16(match mode {
                FileMode::Regular => 1,
                FileMode::Executable => 2,
            })
        }
    }
}

fn read_version(r: &mut CanonicalReader<'_>) -> Result<Version, CodecError> {
    let offset = r.offset();
    match r.read_u16()? {
        0 => Ok(Version::Absent),
        1 => {
            let digest = read_digest(r)?;
            let bytes = r.read_u64()?;
            let mode = match r.read_u16()? {
                1 => FileMode::Regular,
                2 => FileMode::Executable,
                _ => return unknown(offset),
            };
            Ok(Version::Present { digest, bytes, mode })
        }
        _ => unknown(offset),
    }
}

fn write_strings(w: &mut CanonicalWriter, values: &[String]) -> Result<(), CodecError> {
    write_count(w, values.len())?;
    for value in values {
        w.write_str(value)?;
    }
    Ok(())
}

fn read_strings(r: &mut CanonicalReader<'_>, maximum: usize) -> Result<Vec<String>, CodecError> {
    let offset = r.offset();
    let count = read_count(r, offset)?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let value = r.read_str()?;
        if value.len() > maximum {
            return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
        }
        values.push(value.to_owned());
    }
    Ok(values)
}

fn write_count(w: &mut CanonicalWriter, value: usize) -> Result<(), CodecError> {
    w.write_u16(
        u16::try_from(value).map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )
}

fn read_count(r: &mut CanonicalReader<'_>, offset: usize) -> Result<usize, CodecError> {
    let count = usize::from(r.read_u16()?);
    if count > MAX_WORKBENCH_CHECKPOINT_PATHS {
        Err(CodecError::at(CodecErrorKind::LimitExceeded, offset))
    } else {
        Ok(count)
    }
}
