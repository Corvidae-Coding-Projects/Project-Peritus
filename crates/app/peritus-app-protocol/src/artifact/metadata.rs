//! Checked artifact metadata and exact transfer chunks.

use crate::TransferId;
use peritus_types::{ArtifactId, Sha256Digest};

use super::{ArtifactTransferError, ArtifactTransferErrorKind, error::reject};

/// Bounded canonical lowercase Internet media type.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CanonicalMediaType(String);

impl CanonicalMediaType {
    /// Validates a canonical `type/subtype` without parameters or whitespace.
    ///
    /// # Errors
    ///
    /// Rejects a zero byte bound, empty/oversized text, non-ASCII bytes, uppercase text, invalid
    /// token punctuation, or anything other than exactly one nonempty slash separator.
    pub fn new(value: String, maximum_bytes: usize) -> Result<Self, ArtifactTransferError> {
        if maximum_bytes == 0 {
            return Err(reject(
                ArtifactTransferErrorKind::InvalidLimit,
                "media-type limit is zero",
            ));
        }
        if value.is_empty() || value.len() > maximum_bytes || !is_canonical_media_type(&value) {
            return Err(reject(
                ArtifactTransferErrorKind::InvalidInput,
                "media type is not bounded canonical lowercase type/subtype text",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the canonical media type.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_canonical_media_type(value: &str) -> bool {
    let mut slash = None;
    for (index, byte) in value.bytes().enumerate() {
        if byte == b'/' {
            if slash.replace(index).is_some() {
                return false;
            }
        } else if !(byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || matches!(byte, b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-'))
        {
            return false;
        }
    }
    slash.is_some_and(|index| index > 0 && index + 1 < value.len())
}

/// Immutable metadata binding one artifact transfer.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactMetadata {
    transfer_id: TransferId,
    artifact_id: ArtifactId,
    byte_size: u64,
    media_type: CanonicalMediaType,
    digest: Sha256Digest,
    preferred_chunk_size: u32,
}

impl ArtifactMetadata {
    /// Creates exact checked transfer metadata.
    ///
    /// # Errors
    ///
    /// Rejects a zero preferred chunk size or one above the negotiated maximum.
    pub fn new(
        transfer_id: TransferId,
        artifact_id: ArtifactId,
        byte_size: u64,
        media_type: CanonicalMediaType,
        digest: Sha256Digest,
        preferred_chunk_size: u32,
        maximum_chunk_bytes: usize,
    ) -> Result<Self, ArtifactTransferError> {
        let preferred_fits =
            usize::try_from(preferred_chunk_size).is_ok_and(|size| size <= maximum_chunk_bytes);
        if maximum_chunk_bytes == 0 || preferred_chunk_size == 0 || !preferred_fits {
            return Err(reject(
                ArtifactTransferErrorKind::InvalidLimit,
                "preferred chunk size is zero or exceeds the negotiated maximum",
            ));
        }
        Ok(Self { transfer_id, artifact_id, byte_size, media_type, digest, preferred_chunk_size })
    }

    /// Returns the transfer identity.
    #[must_use]
    pub const fn transfer_id(&self) -> TransferId {
        self.transfer_id
    }
    /// Returns the artifact identity.
    #[must_use]
    pub const fn artifact_id(&self) -> ArtifactId {
        self.artifact_id
    }
    /// Returns the exact declared byte size.
    #[must_use]
    pub const fn byte_size(&self) -> u64 {
        self.byte_size
    }
    /// Borrows the canonical media type.
    #[must_use]
    pub const fn media_type(&self) -> &CanonicalMediaType {
        &self.media_type
    }
    /// Returns the declared SHA-256 digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns the preferred nonzero chunk size.
    #[must_use]
    pub const fn preferred_chunk_size(&self) -> u32 {
        self.preferred_chunk_size
    }
}

/// One exact nonempty artifact chunk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactChunk {
    transfer_id: TransferId,
    artifact_id: ArtifactId,
    ordinal: u64,
    offset: u64,
    bytes: Vec<u8>,
}

impl ArtifactChunk {
    /// Creates a chunk under an explicit negotiated byte bound.
    ///
    /// # Errors
    ///
    /// Rejects a zero bound or an empty/oversized byte vector.
    pub fn new(
        transfer_id: TransferId,
        artifact_id: ArtifactId,
        ordinal: u64,
        offset: u64,
        bytes: Vec<u8>,
        maximum_chunk_bytes: usize,
    ) -> Result<Self, ArtifactTransferError> {
        if maximum_chunk_bytes == 0 {
            return Err(reject(ArtifactTransferErrorKind::InvalidLimit, "chunk limit is zero"));
        }
        if bytes.is_empty() || bytes.len() > maximum_chunk_bytes {
            return Err(reject(
                ArtifactTransferErrorKind::InvalidInput,
                "chunk bytes are empty or exceed the negotiated maximum",
            ));
        }
        Ok(Self { transfer_id, artifact_id, ordinal, offset, bytes })
    }

    /// Returns the transfer identity.
    #[must_use]
    pub const fn transfer_id(&self) -> TransferId {
        self.transfer_id
    }
    /// Returns the artifact identity.
    #[must_use]
    pub const fn artifact_id(&self) -> ArtifactId {
        self.artifact_id
    }
    /// Returns the zero-based ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> u64 {
        self.ordinal
    }
    /// Returns the exact byte offset.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }
    /// Borrows the exact opaque chunk bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
