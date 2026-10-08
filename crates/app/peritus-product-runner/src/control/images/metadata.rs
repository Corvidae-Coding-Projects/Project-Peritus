//! Claimed image metadata never substitutes for inspecting original bytes.

use super::{ControlError, ImageFormat};
use crate::attachment::ImageValidation;
use peritus_types::Sha256Digest;

/// Immutable raster metadata, without an assertion that bytes were decoded or imported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageMetadata {
    digest: Sha256Digest,
    bytes: u64,
    format: ImageFormat,
    dimensions: (u32, u32),
    frames: u32,
    validation: ImageValidation,
}
impl ImageMetadata {
    /// Validates historical complete-pixel metadata; the accepting host must verify actual bytes.
    ///
    /// # Errors
    /// Rejects an empty byte, canvas, or frame count.
    pub fn new(
        digest: Sha256Digest,
        bytes: u64,
        format: ImageFormat,
        dimensions: (u32, u32),
        frames: u32,
    ) -> Result<Self, ControlError> {
        Self::new_with_validation(
            digest,
            bytes,
            format,
            dimensions,
            frames,
            ImageValidation::CompletePixels,
        )
    }
    /// Validates representable nonempty metadata with explicit local validation strength.
    ///
    /// # Errors
    /// Rejects an empty byte, canvas, or frame count.
    pub fn new_with_validation(
        digest: Sha256Digest,
        bytes: u64,
        format: ImageFormat,
        dimensions: (u32, u32),
        frames: u32,
        validation: ImageValidation,
    ) -> Result<Self, ControlError> {
        if bytes == 0
            || frames == 0
            || dimensions.0 == 0
            || dimensions.1 == 0
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self { digest, bytes, format, dimensions, frames, validation })
    }
    /// Returns exact original encoded digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    /// Returns exact original encoded size.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }
    /// Returns declared detected format; still requires byte validation.
    #[must_use]
    pub const fn format(self) -> ImageFormat {
        self.format
    }
    /// Returns structurally verified canvas dimensions.
    #[must_use]
    pub const fn dimensions(self) -> (u32, u32) {
        self.dimensions
    }
    /// Returns structurally verified complete frame-record count.
    #[must_use]
    pub const fn frames(self) -> u32 {
        self.frames
    }
    /// Returns the local evidence retained for these exact bytes.
    #[must_use]
    pub const fn validation(self) -> ImageValidation {
        self.validation
    }
}
