//! Raster metadata with explicit local evidence strength.

use super::{AppProtocolError, invalid};
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

/// Local evidence retained for the exact original encoded bytes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WorkbenchImageValidation {
    /// The host decoded every complete pixel frame.
    #[default]
    CompletePixels,
    /// The host streamed and checked the complete encoded container without decoding every pixel.
    ContainerStructure,
}
impl WorkbenchImageValidation {
    /// Returns a stable user-facing description without implying provider delivery.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::CompletePixels => "complete pixels decoded locally",
            Self::ContainerStructure => {
                "complete container inspected locally; selected provider decodes pixels"
            }
        }
    }
}

/// Exact metadata and evidence strength observed by the validating host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchImageMetadata {
    digest: Sha256Digest,
    bytes: u64,
    format: WorkbenchImageFormat,
    dimensions: (u32, u32),
    frames: u32,
    validation: WorkbenchImageValidation,
}
impl WorkbenchImageMetadata {
    /// Validates historical complete-pixel metadata.
    ///
    /// # Errors
    /// Rejects an empty encoded size, canvas, or frame count.
    pub fn new(
        digest: Sha256Digest,
        bytes: u64,
        format: WorkbenchImageFormat,
        dimensions: (u32, u32),
        frames: u32,
    ) -> Result<Self, AppProtocolError> {
        Self::new_with_validation(
            digest,
            bytes,
            format,
            dimensions,
            frames,
            WorkbenchImageValidation::CompletePixels,
        )
    }
    /// Validates representable nonempty metadata with explicit local evidence strength.
    ///
    /// # Errors
    /// Rejects an empty encoded size, canvas, or frame count.
    pub fn new_with_validation(
        digest: Sha256Digest,
        bytes: u64,
        format: WorkbenchImageFormat,
        dimensions: (u32, u32),
        frames: u32,
        validation: WorkbenchImageValidation,
    ) -> Result<Self, AppProtocolError> {
        if bytes == 0
            || frames == 0
            || dimensions.0 == 0
            || dimensions.1 == 0
        {
            return Err(invalid());
        }
        Ok(Self { digest, bytes, format, dimensions, frames, validation })
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
    pub const fn validation(self) -> WorkbenchImageValidation {
        self.validation
    }
}
