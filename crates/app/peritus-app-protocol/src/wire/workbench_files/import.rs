//! Canonical explicit-import transfer, preview and exact consent binding.
use super::{
    CanonicalReader, CanonicalWriter, CodecError, CodecLimits, invalid, read_id, read_metadata,
    read_request, read_string, write_id, write_metadata, write_request,
};
use crate::{
    AppProtocolLimits, WorkbenchFileImportPreview, WorkbenchFileImportRequest, WorkbenchFileUpload,
};
use peritus_types::ArtifactId;

pub(in crate::wire) fn write_upload(
    w: &mut CanonicalWriter,
    value: &WorkbenchFileUpload,
) -> Result<(), CodecError> {
    crate::wire::workbench::write_query(w, value.query())?;
    w.write_u64(value.revision())?;
    crate::wire::artifact::write_artifact_metadata(w, value.metadata())
}
pub(in crate::wire) fn read_upload(
    r: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<WorkbenchFileUpload, CodecError> {
    let offset = r.offset();
    let query = crate::wire::workbench::read_query(r)?;
    let revision = r.read_u64()?;
    invalid(
        offset,
        WorkbenchFileUpload::new(
            query,
            revision,
            crate::wire::artifact::read_artifact_metadata(r, limits)?,
        ),
    )
}
pub(in crate::wire) fn write_import_request(
    w: &mut CanonicalWriter,
    value: &WorkbenchFileImportRequest,
) -> Result<(), CodecError> {
    write_request(w, value.selection())?;
    write_id(w, value.artifact().as_bytes())?;
    write_metadata(w, value.file())
}
pub(in crate::wire) fn read_import_request(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchFileImportRequest, CodecError> {
    let offset = r.offset();
    let selection = read_request(r)?;
    let artifact = read_id(r, ArtifactId::new)?;
    invalid(offset, WorkbenchFileImportRequest::new(selection, artifact, read_metadata(r)?))
}
pub(in crate::wire) fn write_import_preview(
    w: &mut CanonicalWriter,
    value: &WorkbenchFileImportPreview,
) -> Result<(), CodecError> {
    write_import_request(w, value.request())?;
    w.write_u64(value.provider_revision())?;
    w.write_str(value.resolved_model())
}
pub(in crate::wire) fn read_import_preview(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchFileImportPreview, CodecError> {
    let offset = r.offset();
    let request = read_import_request(r)?;
    let revision = r.read_u64()?;
    invalid(offset, WorkbenchFileImportPreview::new(request, revision, read_string(r, 512)?))
}
impl WorkbenchFileImportPreview {
    /// Encodes exact immutable consent, without a source path or read capability.
    ///
    /// # Errors
    /// Rejects encoding limits failures.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CodecError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        write_import_preview(&mut writer, self)?;
        Ok(writer.into_bytes())
    }
    /// Hashes exact imported-source consent.
    ///
    /// # Errors
    /// Rejects encoding limits failures.
    pub fn fingerprint(&self) -> Result<peritus_types::Sha256Digest, CodecError> {
        Ok(peritus_codec::sha256(&self.canonical_bytes()?))
    }
}
