//! Explicit A3/domain image mapping. Checked metadata never substitutes for a decoded proof.

use super::{AppProtocolError, Code, WorkbenchCommand, WorkbenchImagePreview, error};
use crate::product_control::ControlStoreError;
use peritus_app_protocol::{WorkbenchImageFormat as F, WorkbenchImageMetadata};
use peritus_product_runner::{
    attachment::ValidatedImage,
    control::{
        ControlError, ControlText, ImageAttachment, ImageFormat, ImageMetadata, OperationId,
    },
};

pub(super) fn metadata(image: &ValidatedImage) -> Result<WorkbenchImageMetadata, AppProtocolError> {
    let format = match image.media().media_type().as_str() {
        "image/png" => F::Png,
        "image/jpeg" => F::Jpeg,
        "image/gif" => F::Gif,
        "image/webp" => F::Webp,
        _ => return Err(error(Code::MalformedFrame)),
    };
    WorkbenchImageMetadata::new(
        image.digest(),
        image.byte_len(),
        format,
        image.dimensions(),
        image.frames(),
    )
}

pub(in crate::product_run::workbench) fn domain_image(
    command: &WorkbenchCommand,
    preview: &WorkbenchImagePreview,
) -> Result<ImageAttachment, ControlStoreError> {
    if command.query() != preview.request().query()
        || command.expected_revision() != preview.request().revision()
    {
        return Err(ControlError::ScopeMismatch.into());
    }
    let image = preview.image();
    let format = match image.format() {
        F::Png => ImageFormat::Png,
        F::Jpeg => ImageFormat::Jpeg,
        F::Gif => ImageFormat::Gif,
        F::Webp => ImageFormat::Webp,
    };
    let metadata = ImageMetadata::new(
        image.digest(),
        image.bytes(),
        format,
        image.dimensions(),
        image.frames(),
    )?;
    Ok(ImageAttachment::from_metadata(
        OperationId::new(command.operation().into_bytes())?,
        preview.request().artifact(),
        ControlText::new(preview.request().label().as_str().to_owned())?,
        metadata,
    )?
    .with_preview_digest(preview.fingerprint().map_err(|_| ControlError::InvalidInput)?))
}
