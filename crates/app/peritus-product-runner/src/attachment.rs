//! Validation of explicitly selected immutable image bytes, without filesystem authority.
//!
//! The caller owns path authorization, preview consent, artifact retention, and input selection.
//! This module never discovers files or substitutes text for rejected images.

mod decode;
mod text;
pub use text::ValidatedFileText;
#[cfg(test)]
mod tests;

use peritus_model_protocol::{
    Capability, MediaInput, MediaKind, MediaType, ProtocolLimits, ProviderProfile,
};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Optional decoder allocation policy. `None` delegates allocation failure to the decoder/host.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImageDecodePolicy {
    max_allocation_bytes: Option<u64>,
}

impl ImageDecodePolicy {
    /// Creates an explicit allocation policy; `None` places no synthetic decoder ceiling.
    #[must_use]
    pub const fn new(max_allocation_bytes: Option<u64>) -> Self {
        Self { max_allocation_bytes }
    }

    /// Returns the caller-selected decoder allocation ceiling.
    #[must_use]
    pub const fn max_allocation_bytes(self) -> Option<u64> {
        self.max_allocation_bytes
    }
}

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
    /// Detects format, validates all frames incrementally, and retains original bytes.
    ///
    /// # Errors
    /// Rejects missing provider capability or invalid/unsupported image data.
    pub fn decode(bytes: Vec<u8>, profile: &ProviderProfile) -> Result<Self, ProductRunnerError> {
        Self::decode_with_policy(bytes, profile, ImageDecodePolicy::default())
    }

    /// Decodes with an explicit optional allocation policy and provider capacity.
    ///
    /// # Errors
    /// Rejects missing provider capability, an image over provider capacity, or invalid image data.
    pub fn decode_with_policy(
        bytes: Vec<u8>,
        profile: &ProviderProfile,
        policy: ImageDecodePolicy,
    ) -> Result<Self, ProductRunnerError> {
        check_provider(profile)?;
        Self::decode_original_with_policy(bytes, policy)
    }

    /// Validates exact original image bytes without binding admission to a provider profile.
    ///
    /// The resulting image may be archived. Before sending it, callers must apply the selected
    /// provider's capability and per-image capacity policy with [`validate_image_selection`].
    ///
    /// # Errors
    /// Rejects an empty or unsupported image or a failed complete-frame validation.
    pub fn decode_original_with_policy(
        bytes: Vec<u8>,
        policy: ImageDecodePolicy,
    ) -> Result<Self, ProductRunnerError> {
        let byte_len = u64::try_from(bytes.len()).map_err(|_| invalid("image length overflow"))?;
        if byte_len == 0 {
            return Err(invalid("image is empty"));
        }
        let decoded = decode::validate(&bytes, policy)?;
        let digest = Sha256Digest::new(Sha256::digest(&bytes).into());
        let media = MediaInput::inline(
            MediaKind::Image,
            MediaType::new(decoded.mime.to_owned()).map_err(|error| invalid(error.to_string()))?,
            bytes,
            ProtocolLimits::ARCHIVE,
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

    /// Consumes the validation proof and returns the exact original encoded bytes.
    #[must_use]
    pub fn into_original_bytes(self) -> Option<Vec<u8>> {
        self.media.into_inline_bytes()
    }
}

/// Revalidates a complete selection against current provider capability.
/// The caller must apply this to the whole selected request, not independent batches.
///
/// # Errors
/// Rejects unsupported media or a provider-capacity breach. Nothing is omitted.
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
