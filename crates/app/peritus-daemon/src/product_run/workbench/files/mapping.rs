//! Pure A3/source binding; no client can manufacture a host read proof.
use super::{ControlError, Error, WorkbenchCommand, WorkbenchFilePreview};
use peritus_product_runner::control::{
    FileAttachment, FileMode, FileObservation, FileRange, FileSource, FileVersion, OperationId,
};

pub(in crate::product_run::workbench) fn domain_file(
    command: &WorkbenchCommand,
    preview: &WorkbenchFilePreview,
) -> Result<FileAttachment, Error> {
    if command.query() != preview.request().query()
        || command.expected_revision() != preview.request().revision()
    {
        return Err(ControlError::ScopeMismatch.into());
    }
    let request = preview.request();
    let range = match request.range() {
        peritus_app_protocol::WorkbenchFileRange::All => FileRange::All,
        peritus_app_protocol::WorkbenchFileRange::Bytes { start, end } => {
            FileRange::Bytes { start, end }
        }
        peritus_app_protocol::WorkbenchFileRange::Lines { first, last } => {
            FileRange::Lines { first, last }
        }
    };
    let mode = match request.mode() {
        peritus_app_protocol::WorkbenchFileMode::Snapshot => FileMode::Snapshot,
        peritus_app_protocol::WorkbenchFileMode::RefreshOnRequest => FileMode::RefreshOnRequest,
    };
    let path = peritus_patch::WorkspacePath::new(request.path())
        .map_err(|_| ControlError::InvalidInput)?;
    let source = FileSource::workspace(preview.folder(), &path, range, mode)?;
    let file = preview.file();
    let metadata = FileObservation::new(
        file.source_digest(),
        file.source_bytes(),
        file.range(),
        file.digest(),
    )?;
    let version = FileVersion::new(
        OperationId::new(command.operation().into_bytes())?,
        peritus_types::ArtifactId::new(command.operation().into_bytes())
            .map_err(|_| ControlError::InvalidInput)?,
        metadata,
        preview.fingerprint().map_err(|_| ControlError::InvalidInput)?,
    )?;
    Ok(FileAttachment::new(source, version)?)
}
