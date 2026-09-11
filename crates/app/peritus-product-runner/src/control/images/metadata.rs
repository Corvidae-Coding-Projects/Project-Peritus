//! Claimed image metadata is bounded but never substitutes for decoded original bytes.

use super::{
    ControlError, ImageFormat, MAX_IMAGE_BYTES, MAX_IMAGE_FRAMES, MAX_IMAGE_PIXELS, MAX_IMAGE_SIDE,
};
use peritus_types::Sha256Digest;

/// Bounded immutable raster metadata, without an assertion that bytes were decoded or imported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageMetadata {
    digest: Sha256Digest,
    bytes: u64,
    format: ImageFormat,
    dimensions: (u32, u32),
    frames: u32,
}
impl ImageMetadata {
    /// Validates host metadata ceilings only; the accepting host must also verify actual bytes.
    ///
    /// # Errors
    /// Rejects zero or excessive byte, canvas, pixel and frame counts.
    pub fn new(
        digest: Sha256Digest,
        bytes: u64,
        format: ImageFormat,
        dimensions: (u32, u32),
        frames: u32,
    ) -> Result<Self, ControlError> {
        if bytes == 0
            || bytes > MAX_IMAGE_BYTES
            || frames == 0
            || frames > MAX_IMAGE_FRAMES
            || dimensions.0 == 0
            || dimensions.1 == 0
            || dimensions.0 > MAX_IMAGE_SIDE
            || dimensions.1 > MAX_IMAGE_SIDE
            || u64::from(dimensions.0) * u64::from(dimensions.1) > MAX_IMAGE_PIXELS
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self { digest, bytes, format, dimensions, frames })
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
    /// Returns canvas dimensions.
    #[must_use]
    pub const fn dimensions(self) -> (u32, u32) {
        self.dimensions
    }
    /// Returns complete frame count.
    #[must_use]
    pub const fn frames(self) -> u32 {
        self.frames
    }
}
