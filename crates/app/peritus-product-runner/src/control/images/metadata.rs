//! Claimed image metadata is bounded but never substitutes for decoded original bytes.

use super::{ControlError, ImageFormat};
use peritus_types::Sha256Digest;

/// Immutable raster metadata, without an assertion that bytes were decoded or imported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageMetadata {
    digest: Sha256Digest,
    bytes: u64,
    format: ImageFormat,
    dimensions: (u32, u32),
    frames: u32,
}
impl ImageMetadata {
    /// Validates structurally possible metadata; the accepting host must also verify actual bytes.
    ///
    /// # Errors
    /// Rejects empty encoded media or zero dimensions/frames.
    pub const fn new(
        digest: Sha256Digest,
        bytes: u64,
        format: ImageFormat,
        dimensions: (u32, u32),
        frames: u32,
    ) -> Result<Self, ControlError> {
        if bytes == 0 || frames == 0 || dimensions.0 == 0 || dimensions.1 == 0 {
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
