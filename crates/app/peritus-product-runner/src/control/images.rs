//! Explicit immutable image references and future-request selection, not filesystem grants.

use super::{
    ControlError, ControlText, InputId, InputLedger, InputSelection, OperationId, QueueIntent,
};
use crate::attachment::{
    MAX_IMAGE_BYTES, MAX_IMAGE_COUNT, MAX_IMAGE_FRAMES, MAX_IMAGE_PIXELS,
    MAX_IMAGE_SELECTION_BYTES, MAX_IMAGE_SIDE, ValidatedImage,
};
use peritus_types::{ActorId, ArtifactId, Sha256Digest};
use serde::Deserialize;
use serde::Serialize;

mod metadata;
#[cfg(test)]
mod tests;
pub use metadata::ImageMetadata;

/// Detected format of validated raster bytes; not an extension or a client MIME claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    /// Portable Network Graphics, optionally animated.
    Png,
    /// JPEG raster.
    Jpeg,
    /// Graphics Interchange Format, optionally animated.
    Gif,
    /// WebP raster, optionally animated.
    Webp,
}
impl ImageFormat {
    /// Returns the canonical MIME type of the verified encoded bytes.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }
}

/// Immutable image artifact metadata. Owning conversation supplies visibility and authority scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageAttachment {
    operation: OperationId,
    input: InputId,
    artifact: [u8; 16],
    digest: [u8; 32],
    bytes: u64,
    format: ImageFormat,
    width: u32,
    height: u32,
    frames: u32,
    label: ControlText<1024>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preview_digest: Option<[u8; 32]>,
}
impl ImageAttachment {
    /// Captures the original validated bytes' metadata with a user-selected inert source label.
    ///
    /// # Errors
    /// Rejects an unsupported format or invalid source binding. This does not publish an artifact.
    pub fn from_validated(
        operation: OperationId,
        artifact: ArtifactId,
        label: ControlText<1024>,
        image: &ValidatedImage,
    ) -> Result<Self, ControlError> {
        let format = match image.media().media_type().as_str() {
            "image/png" => ImageFormat::Png,
            "image/jpeg" => ImageFormat::Jpeg,
            "image/gif" => ImageFormat::Gif,
            "image/webp" => ImageFormat::Webp,
            _ => return Err(ControlError::InvalidInput),
        };
        let (width, height) = image.dimensions();
        let value = Self {
            operation,
            input: source_input(operation)?,
            artifact: artifact.into_bytes(),
            digest: image.digest().into_bytes(),
            bytes: image.byte_len(),
            format,
            width,
            height,
            frames: image.frames(),
            label,
            preview_digest: None,
        };
        value.validate()?;
        Ok(value)
    }
    /// Constructs checked metadata, not a decoded-image proof or an import receipt.
    /// The host must match this against validated bytes before atomically accepting it.
    ///
    /// # Errors
    /// Rejects invalid immutable source metadata.
    pub fn from_metadata(
        operation: OperationId,
        artifact: ArtifactId,
        label: ControlText<1024>,
        metadata: ImageMetadata,
    ) -> Result<Self, ControlError> {
        let (width, height) = metadata.dimensions();
        let value = Self {
            operation,
            input: source_input(operation)?,
            artifact: artifact.into_bytes(),
            digest: metadata.digest().into_bytes(),
            bytes: metadata.bytes(),
            format: metadata.format(),
            width,
            height,
            frames: metadata.frames(),
            label,
            preview_digest: None,
        };
        value.validate()?;
        Ok(value)
    }
    /// Binds the exact user-confirmed preview archive. The host must install that archive.
    #[must_use]
    pub const fn with_preview_digest(mut self, digest: Sha256Digest) -> Self {
        self.preview_digest = Some(digest.into_bytes());
        self
    }
    /// Returns the original confirmed preview digest, when this import used a preview workflow.
    #[must_use]
    pub const fn preview_digest(&self) -> Option<Sha256Digest> {
        match self.preview_digest {
            Some(bytes) => Some(Sha256Digest::new(bytes)),
            None => None,
        }
    }
    /// Returns the immutable import operation, independent of later selection operations.
    #[must_use]
    pub const fn operation(&self) -> OperationId {
        self.operation
    }
    /// Returns the user-input identity carrying the image's caption and queue lifecycle.
    #[must_use]
    pub const fn input(&self) -> InputId {
        self.input
    }
    /// Returns the original artifact identity bytes, scoped by the owning conversation.
    #[must_use]
    pub const fn artifact_bytes(&self) -> &[u8; 16] {
        &self.artifact
    }
    /// Returns the immutable original byte digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        Sha256Digest::new(self.digest)
    }
    /// Returns exact encoded size.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
    /// Returns detected image format.
    #[must_use]
    pub const fn format(&self) -> ImageFormat {
        self.format
    }
    /// Returns validated canvas dimensions.
    #[must_use]
    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    /// Returns validated frame count.
    #[must_use]
    pub const fn frames(&self) -> u32 {
        self.frames
    }
    /// Borrows the explicit source label, not a filesystem path capability.
    #[must_use]
    pub fn label(&self) -> &str {
        self.label.as_str()
    }
    /// Checks that a decoded immutable artifact is exactly the referenced image.
    #[must_use]
    pub fn matches(&self, image: &ValidatedImage) -> bool {
        self.digest() == image.digest()
            && self.bytes == image.byte_len()
            && self.dimensions() == image.dimensions()
            && self.frames == image.frames()
            && self.format.media_type() == image.media().media_type().as_str()
    }
    pub(super) fn validate(&self) -> Result<(), ControlError> {
        if self.artifact == [0; 16]
            || self.input != source_input(self.operation)?
            || self.bytes == 0
            || self.bytes > MAX_IMAGE_BYTES
            || self.frames == 0
            || self.frames > MAX_IMAGE_FRAMES
            || self.width == 0
            || self.height == 0
            || self.width > MAX_IMAGE_SIDE
            || self.height > MAX_IMAGE_SIDE
            || u64::from(self.width) * u64::from(self.height) > MAX_IMAGE_PIXELS
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(())
    }
}

/// Current explicit selection with immutable import provenance retained even after exclusion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageSelection {
    image: ImageAttachment,
    selected: bool,
}
impl ImageSelection {
    /// Borrows the original immutable reference.
    #[must_use]
    pub const fn image(&self) -> &ImageAttachment {
        &self.image
    }
    /// Reports explicit inclusion preference; held/withdrawn input still prevents eligibility.
    #[must_use]
    pub const fn selected(&self) -> bool {
        self.selected
    }
}

/// Bounded imported-image history and current inclusion preferences.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageAttachments {
    entries: Vec<ImageSelection>,
}
impl ImageAttachments {
    /// Reports absence of all image imports, including excluded history.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Borrows all retained references and their current explicit selection state.
    #[must_use]
    pub fn entries(&self) -> &[ImageSelection] {
        &self.entries
    }
    /// Selects images whose current input revision was included in the exact capture.
    #[must_use]
    pub fn eligible(&self, included: &[InputSelection]) -> Vec<&ImageAttachment> {
        self.entries
            .iter()
            .filter(|entry| {
                entry.selected && included.iter().any(|input| input.id() == entry.image.input())
            })
            .map(|entry| &entry.image)
            .collect()
    }
    pub(super) fn attach(
        &mut self,
        inputs: &mut InputLedger,
        actor: ActorId,
        image: &ImageAttachment,
        text: &ControlText<8192>,
    ) -> Result<(), ControlError> {
        image.validate()?;
        if self.entries.len() >= 256
            || self.entries.iter().any(|entry| entry.image.operation == image.operation)
        {
            return Err(ControlError::Capacity);
        }
        *inputs = inputs.apply(
            actor,
            &QueueIntent::Enqueue { id: image.input, text: text.clone(), dependencies: Vec::new() },
        )?;
        self.entries.push(ImageSelection { image: image.clone(), selected: true });
        self.validate(inputs)
    }
    pub(super) fn select(
        &mut self,
        inputs: &mut InputLedger,
        operation: OperationId,
        selected: bool,
    ) -> Result<(), ControlError> {
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.image.operation == operation)
            .ok_or(ControlError::NotFound)?;
        entry.selected = selected;
        *inputs = inputs.context_changed()?;
        self.validate(inputs)
    }
    pub(super) fn validate(&self, inputs: &InputLedger) -> Result<(), ControlError> {
        let capture = inputs.capture()?;
        let mut operations = std::collections::BTreeSet::new();
        let mut bytes = 0_u64;
        let mut count = 0_usize;
        if self.entries.len() > 256 {
            return Err(ControlError::Capacity);
        }
        for entry in &self.entries {
            entry.image.validate()?;
            if !operations.insert(entry.image.operation)
                || inputs.latest(entry.image.input).is_none()
            {
                return Err(ControlError::InvalidInput);
            }
            if entry.selected
                && capture.included().iter().any(|input| input.id() == entry.image.input)
            {
                count += 1;
                bytes = bytes.checked_add(entry.image.bytes).ok_or(ControlError::Capacity)?;
            }
        }
        if count > MAX_IMAGE_COUNT || bytes > MAX_IMAGE_SELECTION_BYTES {
            return Err(ControlError::Capacity);
        }
        Ok(())
    }
}

fn source_input(operation: OperationId) -> Result<InputId, ControlError> {
    let mut bytes = b"peritus-workbench/image-input/v1".to_vec();
    bytes.extend_from_slice(operation.as_bytes());
    let digest = peritus_codec::sha256(&bytes);
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id[0] |= 1;
    InputId::new(id)
}
