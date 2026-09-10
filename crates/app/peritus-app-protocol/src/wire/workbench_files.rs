//! Canonical typed file preview and selection codecs. Paths are inert descriptors.

use super::primitive::{invalid, read_digest, read_id, unknown, write_digest, write_id};
use crate::{
    WorkbenchFileMetadata, WorkbenchFileMode as M, WorkbenchFilePreview, WorkbenchFileRange as R,
    WorkbenchFileRequest,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind, CodecLimits};

mod import;
mod page;
pub(super) use import::{
    read_import_preview, read_import_request, read_upload, write_import_preview,
    write_import_request, write_upload,
};
pub(super) use page::{read_page, read_query, write_page, write_query};

fn read_string(r: &mut CanonicalReader<'_>, maximum: usize) -> Result<String, CodecError> {
    let offset = r.offset();
    let text = r.read_str()?;
    if text.len() > maximum {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    Ok(text.to_owned())
}
fn write_mode(w: &mut CanonicalWriter, mode: M) -> Result<(), CodecError> {
    w.write_u16(match mode {
        M::Snapshot => 1,
        M::RefreshOnRequest => 2,
    })
}
fn read_mode(r: &mut CanonicalReader<'_>) -> Result<M, CodecError> {
    let offset = r.offset();
    match r.read_u16()? {
        1 => Ok(M::Snapshot),
        2 => Ok(M::RefreshOnRequest),
        _ => unknown(offset),
    }
}
pub(super) fn write_request(
    w: &mut CanonicalWriter,
    value: &WorkbenchFileRequest,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    w.write_u64(value.revision())?;
    w.write_str(value.path())?;
    match value.range() {
        R::All => w.write_u16(1)?,
        R::Bytes { start, end } => {
            w.write_u16(2)?;
            w.write_u64(start)?;
            w.write_u64(end)?;
        }
        R::Lines { first, last } => {
            w.write_u16(3)?;
            w.write_u32(first)?;
            w.write_u32(last)?;
        }
    }
    write_mode(w, value.mode())?;
    write_id(w, value.provider().as_bytes())?;
    w.write_str(value.model().id())?;
    w.write_option_tag(value.model().manual())?;
    w.write_u16(value.model().effort().tag())
}
pub(super) fn read_request(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchFileRequest, CodecError> {
    let offset = r.offset();
    let query = super::workbench::read_query(r)?;
    let revision = r.read_u64()?;
    let path = read_string(r, 4096)?;
    let range = match r.read_u16()? {
        1 => R::All,
        2 => R::Bytes { start: r.read_u64()?, end: r.read_u64()? },
        3 => R::Lines { first: r.read_u32()?, last: r.read_u32()? },
        _ => return unknown(offset),
    };
    let mode = read_mode(r)?;
    let provider = read_id(r, peritus_types::ProviderProfileId::new)?;
    let model = super::interaction::read_choice(r, true)?;
    invalid(offset, WorkbenchFileRequest::new(query, revision, path, range, mode, provider, model))
}
fn write_metadata(w: &mut CanonicalWriter, value: WorkbenchFileMetadata) -> Result<(), CodecError> {
    write_digest(w, value.source_digest())?;
    w.write_u64(value.source_bytes())?;
    w.write_u64(value.range().0)?;
    w.write_u64(value.range().1)?;
    write_digest(w, value.digest())
}
fn read_metadata(r: &mut CanonicalReader<'_>) -> Result<WorkbenchFileMetadata, CodecError> {
    let offset = r.offset();
    let source_digest = read_digest(r)?;
    let source_bytes = r.read_u64()?;
    let range = (r.read_u64()?, r.read_u64()?);
    invalid(offset, WorkbenchFileMetadata::new(source_digest, source_bytes, range, read_digest(r)?))
}
pub(super) fn write_preview(
    w: &mut CanonicalWriter,
    value: &WorkbenchFilePreview,
) -> Result<(), CodecError> {
    write_request(w, value.request())?;
    write_digest(w, value.folder())?;
    write_metadata(w, value.file())?;
    w.write_u64(value.provider_revision())?;
    w.write_str(value.resolved_model())
}
pub(super) fn read_preview(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchFilePreview, CodecError> {
    let offset = r.offset();
    let request = read_request(r)?;
    let folder = read_digest(r)?;
    let file = read_metadata(r)?;
    let revision = r.read_u64()?;
    let model = read_string(r, 512)?;
    invalid(offset, WorkbenchFilePreview::new(request, folder, file, revision, model))
}
impl WorkbenchFilePreview {
    /// Encodes the exact preview for immutable consent archival.
    ///
    /// # Errors
    /// Rejects encoding bounds failures.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CodecError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        write_preview(&mut writer, self)?;
        Ok(writer.into_bytes())
    }
    /// Hashes the exact preview; the digest itself grants no authority.
    ///
    /// # Errors
    /// Rejects encoding bounds failures.
    pub fn fingerprint(&self) -> Result<peritus_types::Sha256Digest, CodecError> {
        Ok(peritus_codec::sha256(&self.canonical_bytes()?))
    }
}
