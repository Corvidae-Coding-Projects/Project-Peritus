//! Validation of explicitly selected immutable image bytes, without filesystem authority.
//!
//! The caller owns path authorization, preview consent, artifact retention, and input selection.
//! This module never discovers files or substitutes text for rejected images.

mod decode;
mod text;
pub use text::{MAX_FILE_BYTES, MAX_FILE_COUNT, MAX_FILE_SELECTION_BYTES, ValidatedFileText};
#[cfg(test)]
mod tests;

use peritus_model_protocol::{
    Capability, MediaInput, MediaKind, MediaType, ProtocolLimits, ProviderProfile,
};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Maximum encoded bytes per explicit image, further constrained by the provider.
pub const MAX_IMAGE_BYTES: u64 = 4 * 1024 * 1024;
/// Maximum aggregate encoded image bytes per request.
pub const MAX_IMAGE_SELECTION_BYTES: u64 = 12 * 1024 * 1024;
/// Maximum number of explicit images in one request.
pub const MAX_IMAGE_COUNT: usize = 16;
/// Maximum width or height, checked before decoding pixels.
pub const MAX_IMAGE_SIDE: u32 = 8192;
/// Maximum pixels in each decoded image canvas.
pub const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
/// Maximum decoded frames per encoded image, including animation frames.
pub const MAX_IMAGE_FRAMES: u32 = 64;
/// Maximum aggregate decoded output per encoded image (not a process RSS guarantee).
pub const MAX_IMAGE_DECODED_BYTES: u64 = 128 * 1024 * 1024;

/// Exact original bytes whose detected format and complete pixel frames passed validation.
/// No public constructor or deserializer can manufacture validation from a claimed digest.
#[derive(Clone, Debug)]
pub struct ValidatedImage {
    media: MediaInput,
    digest: Sha256Digest,
    width: u32,
    height: u32,
    frames: u32,
    byte_len: u64,
}

impl ValidatedImage {
    /// Detects format from bytes and decodes all frames under host and provider bounds.
    ///
    /// # Errors
    /// Rejects missing provider capability, invalid/unsupported image data, or any limit breach.
    pub fn decode(bytes: Vec<u8>, profile: &ProviderProfile) -> Result<Self, ProductRunnerError> {
        check_provider(profile)?;
        let byte_len = u64::try_from(bytes.len()).map_err(|_| invalid("image length overflow"))?;
        if byte_len == 0
            || byte_len > MAX_IMAGE_BYTES.min(profile.limits().max_inline_media_bytes())
        {
            return Err(invalid(
                "image exceeds the host or selected provider byte limit, or is empty",
            ));
        }
        let decoded = decode::validate(&bytes)?;
        let digest = Sha256Digest::new(Sha256::digest(&bytes).into());
        let media = MediaInput::inline(
            MediaKind::Image,
            MediaType::new(decoded.mime.to_owned()).map_err(|error| invalid(error.to_string()))?,
            bytes,
            ProtocolLimits::PRODUCTION,
        )
        .map_err(|error| invalid(error.to_string()))?;
        Ok(Self {
            media,
            digest,
            width: decoded.width,
            height: decoded.height,
            frames: decoded.frames,
            byte_len,
        })
    }

    /// Returns the immutable original provider media, never a re-encoded thumbnail.
    #[must_use]
    pub const fn media(&self) -> &MediaInput {
        &self.media
    }
    /// Returns the original encoded content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns the decoded canvas width and height.
    #[must_use]
    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    /// Returns the verified number of complete frames.
    #[must_use]
    pub const fn frames(&self) -> u32 {
        self.frames
    }
    /// Returns the original encoded byte count.
    #[must_use]
    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }
}

/// Revalidates a complete selection against current provider capability and aggregate ceilings.
/// The caller must apply this to the whole selected request, not independent batches.
///
/// # Errors
/// Rejects unsupported media or an individual, count, or aggregate limit breach. Nothing is omitted.
pub fn validate_image_selection(
    images: &[ValidatedImage],
    profile: &ProviderProfile,
) -> Result<(), ProductRunnerError> {
    if images.is_empty() {
        return Ok(());
    }
    check_provider(profile)?;
    if images.len() > MAX_IMAGE_COUNT {
        return Err(invalid("image selection exceeds the image count limit"));
    }
    let mut total = 0_u64;
    for image in images {
        if image.byte_len > profile.limits().max_inline_media_bytes() {
            return Err(invalid("selected image exceeds the current provider byte limit"));
        }
        total = total
            .checked_add(image.byte_len)
            .ok_or_else(|| invalid("image selection length overflow"))?;
    }
    if total > MAX_IMAGE_SELECTION_BYTES {
        return Err(invalid("image selection exceeds the aggregate byte limit"));
    }
    Ok(())
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
