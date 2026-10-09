//! Bounded raster metadata; a decoded DTO is not an image-validation proof.

use super::{AppProtocolError, MAX_WORKBENCH_IMAGE_BYTES, invalid};
use peritus_types::Sha256Digest;

/// Closed detected raster format, independent of filename extensions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchImageFormat {
    /// PNG raster, possibly animated.
    Png,
    /// JPEG raster.
    Jpeg,
    /// GIF raster, possibly animated.
    Gif,
    /// WebP raster, possibly animated.
    Webp,
}
impl WorkbenchImageFormat {
    /// Returns canonical media type.
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

/// Exact bounded metadata observed by the validating host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchImageMetadata {
    digest: Sha256Digest,
    bytes: u64,
    format: WorkbenchImageFormat,
    dimensions: (u32, u32),
    frames: u32,
}
impl WorkbenchImageMetadata {
    /// Validates protocol ceilings; the host additionally decodes the original artifact.
    ///
    /// # Errors
    /// Rejects zero or excessive encoded size, dimensions, pixels, or frames.
    pub fn new(
        digest: Sha256Digest,
        bytes: u64,
        format: WorkbenchImageFormat,
        dimensions: (u32, u32),
        frames: u32,
    ) -> Result<Self, AppProtocolError> {
        if bytes == 0
            || bytes > MAX_WORKBENCH_IMAGE_BYTES
            || frames == 0
            || frames > 64
            || dimensions.0 == 0
            || dimensions.1 == 0
            || dimensions.0 > 8192
            || dimensions.1 > 8192
            || u64::from(dimensions.0) * u64::from(dimensions.1) > 16 * 1024 * 1024
        {
            return Err(invalid());
        }
        Ok(Self { digest, bytes, format, dimensions, frames })
    }
    /// Returns digest of original encoded bytes, not a thumbnail.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    /// Returns original encoded size.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }
    /// Returns detected format.
    #[must_use]
    pub const fn format(self) -> WorkbenchImageFormat {
        self.format
    }
    /// Returns decoded canvas dimensions.
    #[must_use]
    pub const fn dimensions(self) -> (u32, u32) {
        self.dimensions
    }
    /// Returns validated complete frame count.
    #[must_use]
    pub const fn frames(self) -> u32 {
        self.frames
    }
}
