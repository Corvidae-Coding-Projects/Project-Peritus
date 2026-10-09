//! Immutable image bytes published in the same C0 transaction as their input reference.

use super::{ControlError, ControlOperation, ControlReceipt, ControlStore, Error};
use peritus_journal::StateInstall;
use peritus_model_protocol::{MediaInput, MediaKind, MediaType, ProtocolLimits};
use peritus_product_runner::{
    attachment::{MAX_IMAGE_BYTES, ValidatedImage},
    control::{ControlIntent, ImageAttachment},
};

const IMAGE_NAMESPACE: u16 = 3407;
const PREVIEW_NAMESPACE: u16 = 3408;

impl ControlStore {
    /// Imports already authorized and decoded bytes atomically with their conversation input.
    /// The host must validate artifact ownership before obtaining the decoded image; clients
    /// cannot supply a validation proof. No file is opened and no provider is invoked here.
    pub fn accept_image(
        &mut self,
        operation: &ControlOperation,
        validated: &ValidatedImage,
    ) -> Result<ControlReceipt, Error> {
        self.accept_image_archived(operation, validated, None)
    }

    /// Publishes the exact confirmed preview beside the original image and acceptance receipt.
    pub fn accept_previewed_image(
        &mut self,
        operation: &ControlOperation,
        validated: &ValidatedImage,
        preview: Vec<u8>,
    ) -> Result<ControlReceipt, Error> {
        self.accept_image_archived(operation, validated, Some(preview))
    }

    fn accept_image_archived(
        &mut self,
        operation: &ControlOperation,
        validated: &ValidatedImage,
        preview: Option<Vec<u8>>,
    ) -> Result<ControlReceipt, Error> {
        let ControlIntent::AttachImage { image, .. } = operation.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        if !image.matches(validated) {
            return Err(ControlError::InvalidInput.into());
        }
        match (image.preview_digest(), &preview) {
            (Some(digest), Some(bytes))
                if !bytes.is_empty()
                    && bytes.len() <= 16 * 1024
                    && peritus_codec::sha256(bytes) == digest => {}
            (None, None) => {}
            _ => return Err(ControlError::InvalidInput.into()),
        }
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        let bytes = validated.media().inline_bytes_for_wire().ok_or(ControlError::InvalidInput)?;
        let mut artifacts = vec![StateInstall::new(
            IMAGE_NAMESPACE,
            operation.id().as_bytes().to_vec(),
            None,
            1,
            bytes.to_vec(),
        )?];
        if let Some(preview) = preview {
            artifacts.push(StateInstall::new(
                PREVIEW_NAMESPACE,
                operation.id().as_bytes().to_vec(),
                None,
                1,
                preview,
            )?);
        }
        self.accept_installs(operation, artifacts)
    }

    pub(super) fn verify_image_archive(
        &self,
        image: &ImageAttachment,
        position: u64,
    ) -> Result<(), Error> {
        let artifact = self
            .journal
            .state_record(IMAGE_NAMESPACE, image.operation().as_bytes())?
            .ok_or(Error::Corrupt("image artifact missing"))?;
        if artifact.revision() != 1 || artifact.producing_position() != position {
            return Err(Error::Corrupt("image was not published with its input reference"));
        }
        verify_bytes(image, artifact.bytes())?;
        if let Some(digest) = image.preview_digest() {
            let preview = self
                .journal
                .state_record(PREVIEW_NAMESPACE, image.operation().as_bytes())?
                .ok_or(Error::Corrupt("confirmed image preview missing"))?;
            if preview.revision() != 1
                || preview.producing_position() != position
                || preview.bytes().is_empty()
                || preview.bytes().len() > 16 * 1024
                || peritus_codec::sha256(preview.bytes()) != digest
            {
                return Err(Error::Corrupt(
                    "image preview differs from its atomic consent binding",
                ));
            }
        }
        Ok(())
    }

    // Called only after capture authenticates the owner and verifies immutable history.
    pub(in crate::product_control) fn image_media(
        &self,
        image: &ImageAttachment,
    ) -> Result<MediaInput, Error> {
        let artifact = self
            .journal
            .state_record(IMAGE_NAMESPACE, image.operation().as_bytes())?
            .ok_or(Error::Corrupt("selected image artifact missing"))?;
        verify_bytes(image, artifact.bytes())?;
        MediaInput::inline(
            MediaKind::Image,
            MediaType::new(image.format().media_type().to_owned())
                .map_err(|_| Error::Corrupt("invalid image format"))?,
            artifact.bytes().to_vec(),
            ProtocolLimits::PRODUCTION,
        )
        .map_err(|_| Error::Corrupt("invalid archived image media"))
    }
}

fn verify_bytes(image: &ImageAttachment, bytes: &[u8]) -> Result<(), Error> {
    if bytes.is_empty()
        || bytes.len() as u64 > MAX_IMAGE_BYTES
        || bytes.len() as u64 != image.bytes()
        || peritus_codec::sha256(bytes) != image.digest()
    {
        return Err(Error::Corrupt("image bytes differ from their immutable reference"));
    }
    Ok(())
}
