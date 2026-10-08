//! Canonical legacy and complete checkpoint and rewind forms.

use super::primitive::{invalid, read_digest, read_id, unknown, write_digest, write_id};
use crate::{
    ControlOperationId, WorkbenchCheckpointName, WorkbenchCheckpointPath,
    WorkbenchCheckpointReceipt, WorkbenchCheckpointReferences, WorkbenchRestoreReceipt,
    WorkbenchRestoreStatus, WorkbenchRewindDisposition, WorkbenchRewindMode, WorkbenchRewindPath,
    WorkbenchRewindPreview, WorkbenchRewindRequest,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind, CodecLimits};
use peritus_types::Sha256Digest;

mod coverage;
mod manifest;
use manifest::ManifestForm;
pub use manifest::{legacy_text, wide_lists};
#[cfg(test)]
mod tests;
use coverage::{read_captured, read_version, write_captured, write_version};

pub(super) fn write_checkpoint_receipt(
    w: &mut CanonicalWriter,
    value: &WorkbenchCheckpointReceipt,
) -> Result<(), CodecError> {
    let form = ManifestForm::for_wide(value.requires_manifest_feature());
    write_id(w, value.checkpoint().as_bytes())?;
    super::workbench::write_query(w, value.query())?;
    w.write_u64(value.accepted_revision())?;
    w.write_str(value.name().as_str())?;
    write_references(w, value.references())?;
    write_checkpoint_paths(w, value.paths(), form)?;
    write_strings(w, value.exclusions(), form)?;
    write_strings(w, value.external_effects(), form)
}

pub(super) fn read_checkpoint_receipt(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchCheckpointReceipt, CodecError> {
    read_checkpoint_receipt_as(r, false)
}

pub(super) fn read_checkpoint_receipt_as(
    r: &mut CanonicalReader<'_>,
    wide: bool,
) -> Result<WorkbenchCheckpointReceipt, CodecError> {
    let form = ManifestForm::for_wide(wide);
    let offset = r.offset();
    let checkpoint = read_id(r, ControlOperationId::new)?;
    let query = super::workbench::read_query(r)?;
    let revision = r.read_u64()?;
    let name = invalid(offset, WorkbenchCheckpointName::new(r.read_str()?.to_owned()))?;
    let references = read_references(r)?;
    let paths = read_checkpoint_paths(r, form)?;
    let exclusions = read_strings(r, form)?;
    let external = read_strings(r, form)?;
    let value = invalid(
        offset,
        WorkbenchCheckpointReceipt::new(
            checkpoint, query, revision, name, references, paths, exclusions, external,
        ),
    )?;
    form.check(value.requires_manifest_feature(), offset)?;
    Ok(value)
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
    invalid(offset, request.with_branch(mode, child))
}

pub(super) fn write_preview(
    w: &mut CanonicalWriter,
    value: &WorkbenchRewindPreview,
) -> Result<(), CodecError> {
    let form = ManifestForm::for_wide(value.requires_manifest_feature());
    write_request(w, value.request())?;
    write_digest(w, value.preview_digest())?;
    write_rewind_paths(w, value.paths(), form)?;
    write_strings(w, value.exclusions(), form)?;
    write_strings(w, value.external_effects(), form)?;
    w.write_bool(value.conversation_history_preserved())?;
    w.write_bool(value.accounting_preserved())
}

pub(super) fn read_preview(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchRewindPreview, CodecError> {
    read_preview_as(r, false)
}

pub(super) fn read_preview_as(
    r: &mut CanonicalReader<'_>,
    wide: bool,
) -> Result<WorkbenchRewindPreview, CodecError> {
    let form = ManifestForm::for_wide(wide);
    let offset = r.offset();
    let request = read_request(r)?;
    let digest = read_digest(r)?;
    let paths = read_rewind_paths(r, form)?;
    let exclusions = read_strings(r, form)?;
    let external = read_strings(r, form)?;
    if !r.read_bool()? || !r.read_bool()? {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    let preview =
        invalid(offset, WorkbenchRewindPreview::new(request, paths, exclusions, external))?;
    form.check(preview.requires_manifest_feature(), offset)?;
    if preview.preview_digest() != digest {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    Ok(preview)
}

pub(super) fn write_restore_receipt(
    w: &mut CanonicalWriter,
    value: &WorkbenchRestoreReceipt,
) -> Result<(), CodecError> {
    let form = ManifestForm::for_wide(value.requires_manifest_feature());
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
    write_strings(w, value.restored(), form)?;
    write_strings(w, value.conflicts(), form)?;
    write_strings(w, value.external_effects(), form)
}

pub(super) fn read_restore_receipt(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchRestoreReceipt, CodecError> {
    read_restore_receipt_as(r, false)
}

pub(super) fn read_restore_receipt_as(
    r: &mut CanonicalReader<'_>,
    wide: bool,
) -> Result<WorkbenchRestoreReceipt, CodecError> {
    let form = ManifestForm::for_wide(wide);
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
    let value = invalid(
        offset,
        WorkbenchRestoreReceipt::new(
            restore,
            checkpoint,
            recovery,
            query,
            revision,
            status,
            read_strings(r, form)?,
            read_strings(r, form)?,
            read_strings(r, form)?,
        ),
    )?;
    form.check(value.requires_manifest_feature(), offset)?;
    Ok(value)
}

pub fn preview_fingerprint(
    request: WorkbenchRewindRequest,
    paths: &[WorkbenchRewindPath],
    exclusions: &[String],
    external: &[String],
) -> Result<Sha256Digest, CodecError> {
    let mut w = CanonicalWriter::new(CodecLimits::PRODUCTION);
    let form = ManifestForm::for_wide(wide_lists(
        paths.iter().map(WorkbenchRewindPath::path),
        exclusions,
        external,
    ));
    w.write_fixed(if form.is_wide() {
        b"peritus-workbench-rewind-preview-v2"
    } else {
        b"peritus-workbench-rewind-preview-v1"
    })?;
    write_request(&mut w, request)?;
    write_rewind_paths(&mut w, paths, form)?;
    write_strings(&mut w, exclusions, form)?;
    write_strings(&mut w, external, form)?;
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
    form: ManifestForm,
) -> Result<(), CodecError> {
    form.write_count(w, paths.len())?;
    for path in paths {
        w.write_str(path.path())?;
        write_captured(w, path.checkpoint(), path.ranges())?;
        w.write_bool(path.expected_current().is_some())?;
        if let Some(version) = path.expected_current() {
            write_version(w, version)?;
        }
    }
    Ok(())
}

fn read_checkpoint_paths(
    r: &mut CanonicalReader<'_>,
    form: ManifestForm,
) -> Result<Vec<WorkbenchCheckpointPath>, CodecError> {
    let offset = r.offset();
    let count = form.read_count(r, 8)?;
    let mut values = r.reserve_collection(count)?;
    for _ in 0..count {
        let path = r.read_str()?.to_owned();
        let (checkpoint, ranges) = read_captured(r)?;
        let expected = if r.read_bool()? { Some(read_version(r)?) } else { None };
        let value = invalid(offset, WorkbenchCheckpointPath::new(path, checkpoint, expected))?;
        values.push(invalid(offset, value.with_ranges(ranges))?);
    }
    Ok(values)
}

fn write_rewind_paths(
    w: &mut CanonicalWriter,
    paths: &[WorkbenchRewindPath],
    form: ManifestForm,
) -> Result<(), CodecError> {
    form.write_count(w, paths.len())?;
    for path in paths {
        w.write_str(path.path())?;
        write_captured(w, path.checkpoint(), path.ranges())?;
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
            WorkbenchRewindDisposition::Unavailable => 5,
        })?;
    }
    Ok(())
}

fn read_rewind_paths(
    r: &mut CanonicalReader<'_>,
    form: ManifestForm,
) -> Result<Vec<WorkbenchRewindPath>, CodecError> {
    let offset = r.offset();
    let count = form.read_count(r, 12)?;
    let mut values = r.reserve_collection(count)?;
    for _ in 0..count {
        let path = r.read_str()?.to_owned();
        let (checkpoint, ranges) = read_captured(r)?;
        let expected = if r.read_bool()? { Some(read_version(r)?) } else { None };
        let observed = read_version(r)?;
        let disposition = match r.read_u16()? {
            1 => WorkbenchRewindDisposition::Restore,
            2 => WorkbenchRewindDisposition::Unchanged,
            3 => WorkbenchRewindDisposition::Conflict,
            4 => WorkbenchRewindDisposition::Unsealed,
            5 => WorkbenchRewindDisposition::Unavailable,
            _ => return unknown(offset),
        };
        values.push(invalid(
            offset,
            WorkbenchRewindPath::new_with_ranges(
                path,
                checkpoint,
                expected,
                observed,
                disposition,
                ranges,
            ),
        )?);
    }
    Ok(values)
}

fn write_strings(
    w: &mut CanonicalWriter,
    values: &[String],
    form: ManifestForm,
) -> Result<(), CodecError> {
    form.write_count(w, values.len())?;
    for value in values {
        w.write_str(value)?;
    }
    Ok(())
}

fn read_strings(
    r: &mut CanonicalReader<'_>,
    form: ManifestForm,
) -> Result<Vec<String>, CodecError> {
    let count = form.read_count(r, 5)?;
    let mut values = r.reserve_collection(count)?;
    for _ in 0..count {
        let value = r.read_str()?;
        values.push(value.to_owned());
    }
    Ok(values)
}
