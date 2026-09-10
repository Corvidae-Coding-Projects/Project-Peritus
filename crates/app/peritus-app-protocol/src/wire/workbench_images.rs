//! Canonical image previews and scoped uploads; original media uses the existing chunk transfer.

use super::primitive::{invalid, read_digest, read_id, unknown, write_digest, write_id};
use crate::{
    AppProtocolLimits, WorkbenchImageFormat as F, WorkbenchImageLabel, WorkbenchImageMetadata,
    WorkbenchImagePreview, WorkbenchImageRequest, WorkbenchImageUpload,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind, CodecLimits};
use peritus_types::{ArtifactId, ProviderProfileId};

pub(super) fn write_upload(
    w: &mut CanonicalWriter,
    value: &WorkbenchImageUpload,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    w.write_u64(value.revision())?;
    super::artifact::write_artifact_metadata(w, value.metadata())
}
pub(super) fn read_upload(
    r: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<WorkbenchImageUpload, CodecError> {
    let offset = r.offset();
    let query = super::workbench::read_query(r)?;
    let revision = r.read_u64()?;
    invalid(
        offset,
        WorkbenchImageUpload::new(
            query,
            revision,
            super::artifact::read_artifact_metadata(r, limits)?,
        ),
    )
}
pub(super) fn write_request(
    w: &mut CanonicalWriter,
    value: &WorkbenchImageRequest,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    w.write_u64(value.revision())?;
    write_id(w, value.artifact().as_bytes())?;
    write_id(w, value.provider().as_bytes())?;
    w.write_str(value.model().id())?;
    w.write_option_tag(value.model().manual())?;
    w.write_u16(value.model().effort().tag())?;
    w.write_str(value.label().as_str())
}
pub(super) fn read_request(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchImageRequest, CodecError> {
    let offset = r.offset();
    let query = super::workbench::read_query(r)?;
    let revision = r.read_u64()?;
    let artifact = read_id(r, ArtifactId::new)?;
    let provider = read_id(r, ProviderProfileId::new)?;
    let model = super::interaction::read_choice(r, true)?;
    let label = r.read_str()?;
    if label.len() > crate::MAX_WORKBENCH_IMAGE_LABEL_BYTES {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let label = invalid(offset, WorkbenchImageLabel::new(label.to_owned()))?;
    invalid(offset, WorkbenchImageRequest::new(query, revision, artifact, provider, model, label))
}
pub(super) fn write_metadata(
    w: &mut CanonicalWriter,
    image: WorkbenchImageMetadata,
) -> Result<(), CodecError> {
    write_digest(w, image.digest())?;
    w.write_u64(image.bytes())?;
    w.write_u16(match image.format() {
        F::Png => 1,
        F::Jpeg => 2,
        F::Gif => 3,
        F::Webp => 4,
    })?;
    w.write_u32(image.dimensions().0)?;
    w.write_u32(image.dimensions().1)?;
    w.write_u32(image.frames())
}
pub(super) fn read_metadata(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchImageMetadata, CodecError> {
    let offset = r.offset();
    let digest = read_digest(r)?;
    let bytes = r.read_u64()?;
    let format = match r.read_u16()? {
        1 => F::Png,
        2 => F::Jpeg,
        3 => F::Gif,
        4 => F::Webp,
        _ => return unknown(offset),
    };
    let dimensions = (r.read_u32()?, r.read_u32()?);
    let frames = r.read_u32()?;
    invalid(offset, WorkbenchImageMetadata::new(digest, bytes, format, dimensions, frames))
}
pub(super) fn write_preview(
    w: &mut CanonicalWriter,
    value: &WorkbenchImagePreview,
) -> Result<(), CodecError> {
    write_request(w, value.request())?;
    write_metadata(w, value.image())?;
    w.write_u64(value.provider_revision())?;
    w.write_str(value.resolved_model())
}
pub(super) fn read_preview(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchImagePreview, CodecError> {
    let offset = r.offset();
    let request = read_request(r)?;
    let image = read_metadata(r)?;
    let revision = r.read_u64()?;
    let model = r.read_str()?;
    if model.len() > 512 {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    invalid(offset, WorkbenchImagePreview::new(request, image, revision, model.to_owned()))
}
impl WorkbenchImagePreview {
    /// Returns exact bounded preview bytes for atomic consent archival and idempotency binding.
    ///
    /// # Errors
    /// Rejects any canonical encoding limit violation.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CodecError> {
        let mut w = CanonicalWriter::new(CodecLimits::PRODUCTION);
        write_preview(&mut w, self)?;
        Ok(w.into_bytes())
    }
    /// Returns the exact consent-preview digest; this is not an authority token.
    ///
    /// # Errors
    /// Rejects an encoding failure.
    pub fn fingerprint(&self) -> Result<peritus_types::Sha256Digest, CodecError> {
        Ok(peritus_codec::sha256(&self.canonical_bytes()?))
    }
}
