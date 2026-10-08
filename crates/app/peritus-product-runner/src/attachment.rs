//! Validation of explicitly selected immutable image bytes, without filesystem authority.
//!
//! The caller owns path authorization, preview consent, artifact retention, and input selection.
//! This module never discovers files or substitutes text for rejected images.

mod decode;
mod text;
pub use text::{MAX_FILE_BYTES, ValidatedFileText, ValidatedFileTextSource};
#[cfg(test)]
mod tests;

use peritus_model_protocol::{
    Capability, MediaInput, MediaKind, MediaType, ProviderProfile,
};
use peritus_types::{ArtifactId, Sha256Digest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Legacy compatibility value; production image admission uses the selected provider profile.
#[deprecated(note = "use the selected provider's max_inline_media_bytes")]
pub const MAX_IMAGE_BYTES: u64 = 4 * 1024 * 1024;
/// Legacy compatibility value; production selection has no host aggregate media allowance.
#[deprecated(note = "aggregate image admission belongs to the selected provider")]
pub const MAX_IMAGE_SELECTION_BYTES: u64 = 12 * 1024 * 1024;
/// Legacy compatibility value; production selection has no host image-count allowance.
#[deprecated(note = "image count admission belongs to the selected provider")]
pub const MAX_IMAGE_COUNT: usize = 16;
/// Legacy compatibility value; dimensions are no longer an attachment policy.
#[deprecated(note = "decoder allocations are bounded independently of image dimensions")]
pub const MAX_IMAGE_SIDE: u32 = 8192;
/// Legacy compatibility value; pixel count is no longer an attachment policy.
#[deprecated(note = "decoder allocations are bounded independently of pixel count")]
pub const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
/// Legacy compatibility value; frame count is no longer an attachment policy.
#[deprecated(note = "animation frame admission belongs to the selected provider")]
pub const MAX_IMAGE_FRAMES: u32 = 64;
/// Maximum physical allocation window supplied to the optional pixel decoder.
///
/// This is an allocation-safety window rather than an attachment byte, dimension, pixel, frame,
/// or total-animation policy. Images whose complete pixel output does not fit are admitted from
/// streamed container evidence and retain that distinction in [`ImageValidation`].
pub const MAX_IMAGE_DECODE_ALLOCATION_BYTES: u64 = 128 * 1024 * 1024;
/// Compatibility alias for the former cumulative-output name.
#[deprecated(note = "use MAX_IMAGE_DECODE_ALLOCATION_BYTES")]
pub const MAX_IMAGE_DECODED_BYTES: u64 = 128 * 1024 * 1024;

/// Strength of local image evidence retained independently from provider acceptance.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageValidation {
    /// The host decoded every complete pixel frame from the exact original bytes.
    #[default]
    CompletePixels,
    /// The host inspected the complete encoded container without decoding every pixel.
    ///
    /// Format framing, dimensions, frame records, truncation, and available container checksums
    /// were checked. Compressed pixel-stream validity remains the selected provider's responsibility.
    ContainerStructure,
}

impl ImageValidation {
    /// Returns whether the exact original's complete pixel output was decoded locally.
    #[must_use]
    pub const fn complete_pixels(self) -> bool {
        matches!(self, Self::CompletePixels)
    }
}

/// Exact original media identity with explicit local validation strength.
/// No public constructor or deserializer can manufacture this evidence from claimed metadata.
#[derive(Clone, Debug)]
pub struct ValidatedImage {
    media: MediaInput,
    artifact: Option<ArtifactId>,
    digest: Sha256Digest,
    width: u32,
    height: u32,
    frames: u32,
    byte_len: u64,
    validation: ImageValidation,
}

impl ValidatedImage {
    /// Inspects exact in-memory bytes under the provider bound and decodes pixels when the bounded
    /// decoder can do so without making canvas size an admission policy.
    ///
    /// Durable workbench imports use Self::inspect_artifact so the exact original remains an
    /// artifact reference and request archives never duplicate its bytes.
    ///
    /// # Errors
    /// Rejects missing provider capability, an unsupported or malformed image container, corruption
    /// detected by bounded pixel decoding, or the provider's byte limit.
    pub fn decode(bytes: Vec<u8>, profile: &ProviderProfile) -> Result<Self, ProductRunnerError> {
        check_provider(profile)?;
        let byte_len = u64::try_from(bytes.len()).map_err(|_| invalid("image length overflow"))?;
        if byte_len == 0 || byte_len > profile.limits().max_inline_media_bytes() {
            return Err(invalid("image exceeds the selected provider byte limit, or is empty"));
        }
        let mut source = std::io::Cursor::new(bytes.as_slice());
        let decoded = decode::validate(&mut source)?;
        let digest = Sha256Digest::new(Sha256::digest(&bytes).into());
        let media = MediaInput::inline_with_maximum(
            MediaKind::Image,
            MediaType::new(decoded.mime.to_owned()).map_err(|error| invalid(error.to_string()))?,
            bytes,
            profile.limits().max_inline_media_bytes(),
        )
        .map_err(|error| invalid(error.to_string()))?;
        Ok(Self {
            media,
            artifact: None,
            digest,
            width: decoded.width,
            height: decoded.height,
            frames: decoded.frames,
            byte_len,
            validation: decoded.validation,
        })
    }

    /// Validates one exact durable artifact through bounded reads while retaining it by identity.
    ///
    /// The supplied reader is hashed independently before complete container inspection. Pixel
    /// decoding is optional and bounded; its result is reported by [`Self::validation`].
    ///
    /// # Errors
    /// Rejects an empty artifact, digest/length mismatch, unsupported or malformed container,
    /// corruption detected by bounded pixel decoding, absent provider capability, or the selected
    /// provider's actual per-image byte limit.
    pub fn inspect_artifact<R: Read + Seek>(
        reader: &mut R,
        artifact_id: ArtifactId,
        expected_digest: Sha256Digest,
        byte_len: u64,
        profile: &ProviderProfile,
    ) -> Result<Self, ProductRunnerError> {
        check_provider(profile)?;
        if byte_len == 0 || byte_len > profile.limits().max_inline_media_bytes() {
            return Err(invalid("image exceeds the selected provider byte limit, or is empty"));
        }
        let observed = digest_reader(reader, byte_len)?;
        if observed != expected_digest {
            return Err(invalid("image artifact digest differs from its immutable receipt"));
        }
        reader.seek(SeekFrom::Start(0)).map_err(read_error)?;
        let decoded = decode::validate(reader)?;
        let media_type =
            MediaType::new(decoded.mime.to_owned()).map_err(|error| invalid(error.to_string()))?;
        Ok(Self {
            media: MediaInput::artifact(
                MediaKind::Image,
                media_type,
                artifact_id,
                expected_digest,
            ),
            artifact: Some(artifact_id),
            digest: expected_digest,
            width: decoded.width,
            height: decoded.height,
            frames: decoded.frames,
            byte_len,
            validation: decoded.validation,
        })
    }

    /// Returns the immutable original provider media, never a re-encoded thumbnail.
    #[must_use]
    pub const fn media(&self) -> &MediaInput {
        &self.media
    }
    /// Returns the exact durable artifact identity when validation used an artifact reader.
    #[must_use]
    pub const fn artifact(&self) -> Option<ArtifactId> {
        self.artifact
    }
    /// Returns the original encoded content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns the structurally verified canvas width and height.
    #[must_use]
    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    /// Returns the structurally verified number of complete frame records.
    #[must_use]
    pub const fn frames(&self) -> u32 {
        self.frames
    }
    /// Returns the original encoded byte count.
    #[must_use]
    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }
    /// Returns whether local evidence includes complete pixels or complete container structure.
    #[must_use]
    pub const fn validation(&self) -> ImageValidation {
        self.validation
    }
}

/// Revalidates a complete selection against the current provider's actual image capability.
///
/// # Errors
/// Rejects unsupported media or an individual provider byte limit breach. Nothing is omitted.
pub fn validate_image_selection(
    images: &[ValidatedImage],
    profile: &ProviderProfile,
) -> Result<(), ProductRunnerError> {
    if images.is_empty() {
        return Ok(());
    }
    check_provider(profile)?;
    for image in images {
        if image.byte_len > profile.limits().max_inline_media_bytes() {
            return Err(invalid("selected image exceeds the current provider byte limit"));
        }
    }
    Ok(())
}

fn digest_reader<R: Read + Seek>(
    reader: &mut R,
    expected_bytes: u64,
) -> Result<Sha256Digest, ProductRunnerError> {
    reader.seek(SeekFrom::Start(0)).map_err(read_error)?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let count = reader.read(&mut buffer).map_err(read_error)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(u64::try_from(count).map_err(|_| invalid("image length overflow"))?)
            .ok_or_else(|| invalid("image length overflow"))?;
        if total > expected_bytes {
            return Err(invalid("image artifact grew beyond its immutable receipt"));
        }
        hash.update(&buffer[..count]);
    }
    if total != expected_bytes {
        return Err(invalid("image artifact length differs from its immutable receipt"));
    }
    Ok(Sha256Digest::new(hash.finalize().into()))
}

fn read_error(error: std::io::Error) -> ProductRunnerError {
    invalid(format!("read immutable image artifact: {error}"))
}

fn check_provider(profile: &ProviderProfile) -> Result<(), ProductRunnerError> {
    if !profile.capabilities().supports(Capability::ImageInput) {
        return Err(invalid(
            "selected provider cannot accept images; select an image-capable provider",
        ));
    }
    Ok(())
}

fn invalid(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Provider,
        "validate explicit image attachment",
        detail,
    )
}
