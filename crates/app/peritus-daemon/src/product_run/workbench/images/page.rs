//! Read-only retained image metadata from a verified, authenticated conversation snapshot.

use super::ProductRunService;
use crate::product_run::workbench::{error_response, inputs::project_row};
use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, MAX_WORKBENCH_IMAGE_PAGE, WorkbenchImageFormat as F,
    WorkbenchImageLabel, WorkbenchImageMetadata, WorkbenchImagePage, WorkbenchImageQuery,
    WorkbenchImageRow,
};
use peritus_product_runner::control::{ControlError, ConversationId, ImageAttachment, ImageFormat};
use peritus_types::{ActorId, ArtifactId};

impl ProductRunService {
    pub(crate) fn workbench_images(
        &self,
        actor: ActorId,
        query: WorkbenchImageQuery,
    ) -> AppResponsePayload {
        let result = self.control_workspace(query.query()).and_then(|()| {
            let id = ConversationId::new(query.query().conversation().into_bytes())?;
            let record =
                self.with_controls(false, |store| store.load(id))?.ok_or(ControlError::NotFound)?;
            if record.owner_bytes() != actor.as_bytes()
                || record.workspace_bytes() != query.query().workspace().as_bytes()
            {
                return Err(ControlError::ScopeMismatch.into());
            }
            if query.revision() != 0 && query.revision() != record.revision() {
                return Err(ControlError::StaleRevision.into());
            }
            let entries = record.images().entries();
            let total = u32::try_from(entries.len()).map_err(|_| ControlError::Capacity)?;
            if query.offset() > total {
                return Err(ControlError::InvalidInput.into());
            }
            let capture = record.inputs().capture()?;
            let mut rows = Vec::new();
            for entry in entries.iter().skip(query.offset() as usize).take(MAX_WORKBENCH_IMAGE_PAGE)
            {
                let image = entry.image();
                let source =
                    record.inputs().latest(image.input()).ok_or(ControlError::InvalidInput)?;
                let eligible = entry.selected()
                    && capture.included().iter().any(|input| input.id() == image.input());
                rows.push(
                    WorkbenchImageRow::new(
                        ControlOperationId::new(*image.operation().as_bytes())
                            .map_err(|_| ControlError::InvalidInput)?,
                        ArtifactId::new(*image.artifact_bytes())
                            .map_err(|_| ControlError::InvalidInput)?,
                        WorkbenchImageLabel::new(image.label().to_owned())
                            .map_err(|_| ControlError::InvalidInput)?,
                        metadata(image)?,
                        project_row(source)?,
                        (entry.selected(), eligible),
                    )
                    .map_err(|_| ControlError::InvalidInput)?,
                );
            }
            let selected =
                WorkbenchImageQuery::new(query.query(), record.revision(), query.offset())
                    .map_err(|_| ControlError::InvalidInput)?;
            WorkbenchImagePage::new(selected, total, rows)
                .map_err(|_| ControlError::InvalidInput.into())
        });
        result.map_or_else(error_response, AppResponsePayload::WorkbenchImages)
    }
}
fn metadata(image: &ImageAttachment) -> Result<WorkbenchImageMetadata, ControlError> {
    let format = match image.format() {
        ImageFormat::Png => F::Png,
        ImageFormat::Jpeg => F::Jpeg,
        ImageFormat::Gif => F::Gif,
        ImageFormat::Webp => F::Webp,
    };
    WorkbenchImageMetadata::new(
        image.digest(),
        image.bytes(),
        format,
        image.dimensions(),
        image.frames(),
    )
    .map_err(|_| ControlError::InvalidInput)
}
