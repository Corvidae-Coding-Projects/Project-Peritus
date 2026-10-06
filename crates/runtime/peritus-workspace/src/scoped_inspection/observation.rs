//! Versioned metadata and continuation records, separate from retained content and authority.

use super::{WorkspaceError, invalid};
use peritus_codec::{CanonicalReader, CodecLimits, sha256};
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;

const OBSERVATION_FORMAT: &[u8; 8] = b"PINSv001";
const CURSOR_FORMAT: &[u8; 8] = b"PINCv001";

/// Exact source and selection identity from one completed inspection.
///
/// This is historical observation metadata, not consent, mutation authority, or proof of an
/// atomic snapshot against arbitrary concurrent writers. The host owns durable publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectedSelection {
    folder: Sha256Digest,
    path: WorkspacePath,
    source_bytes: u64,
    source_digest: Sha256Digest,
    range: (u64, u64),
    digest: Sha256Digest,
}
impl InspectedSelection {
    pub(super) const fn observed(
        folder: Sha256Digest,
        path: WorkspacePath,
        source_bytes: u64,
        source_digest: Sha256Digest,
        range: (u64, u64),
        digest: Sha256Digest,
    ) -> Self {
        Self { folder, path, source_bytes, source_digest, range, digest }
    }
    /// Returns the root identity observed when the source was inspected.
    #[must_use]
    pub const fn folder_digest(&self) -> Sha256Digest {
        self.folder
    }
    /// Borrows the canonical source path.
    #[must_use]
    pub const fn path(&self) -> &WorkspacePath {
        &self.path
    }
    /// Returns the complete source size, independently of selected content.
    #[must_use]
    pub const fn source_bytes(&self) -> u64 {
        self.source_bytes
    }
    /// Returns SHA-256 of the complete source.
    #[must_use]
    pub const fn source_digest(&self) -> Sha256Digest {
        self.source_digest
    }
    /// Returns the exact half-open source byte interval included in retained storage.
    #[must_use]
    pub const fn range(&self) -> (u64, u64) {
        self.range
    }
    /// Returns the selected content length without loading it.
    #[must_use]
    pub const fn selected_bytes(&self) -> u64 {
        self.range.1 - self.range.0
    }
    /// Returns SHA-256 of the exact selected content.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Encodes version-one metadata with a checksum. Content is stored separately; this small
    /// record's size depends only on the already representable workspace path.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(OBSERVATION_FORMAT);
        bytes.extend_from_slice(self.folder.as_bytes());
        bytes.extend_from_slice(&self.source_bytes.to_be_bytes());
        bytes.extend_from_slice(self.source_digest.as_bytes());
        bytes.extend_from_slice(&self.range.0.to_be_bytes());
        bytes.extend_from_slice(&self.range.1.to_be_bytes());
        bytes.extend_from_slice(self.digest.as_bytes());
        bytes.extend_from_slice(self.path.as_str().as_bytes());
        append_checksum(&mut bytes);
        bytes
    }
    /// Restores metadata persisted by the owner. Opening retained content verifies its exact
    /// size and digest; decoding metadata alone makes no claim about source or owner authority.
    ///
    /// # Errors
    /// Rejects unknown versions, incomplete/changed records, invalid paths or impossible ranges.
    pub fn decode(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        let payload = checked_record(bytes, OBSERVATION_FORMAT)?;
        let fields = payload.get(8..128).ok_or_else(record_error)?;
        let mut reader = CanonicalReader::new(fields, CodecLimits::PRODUCTION);
        let folder = Sha256Digest::new(reader.read_fixed().map_err(|_| record_error())?);
        let source_bytes = reader.read_u64().map_err(|_| record_error())?;
        let source_digest = Sha256Digest::new(reader.read_fixed().map_err(|_| record_error())?);
        let range = (
            reader.read_u64().map_err(|_| record_error())?,
            reader.read_u64().map_err(|_| record_error())?,
        );
        let digest = Sha256Digest::new(reader.read_fixed().map_err(|_| record_error())?);
        reader.finish().map_err(|_| record_error())?;
        let path = payload.get(128..).ok_or_else(record_error)?;
        let path = std::str::from_utf8(path).map_err(|_| record_error())?;
        let path = WorkspacePath::new(path).map_err(|_| record_error())?;
        if range.0 > range.1
            || range.1 > source_bytes
            || (range.0 == range.1 && (range != (0, 0) || digest != sha256(&[])))
            || (source_bytes == 0 && source_digest != sha256(&[]))
            || (range == (0, source_bytes) && digest != source_digest)
        {
            return Err(record_error());
        }
        Ok(Self::observed(folder, path, source_bytes, source_digest, range, digest))
    }
    pub(super) fn binding(&self) -> Sha256Digest {
        sha256(&self.encode())
    }
}

/// A persisted next offset bound to the exact retained source observation.
/// It contains no session, deadline, page counter, or ambient source lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InspectionCursor {
    pub(super) observation: Sha256Digest,
    pub(super) offset: u64,
}
impl InspectionCursor {
    /// Returns the next offset relative to the retained selection.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }
    /// Encodes a version-one cursor with its observation binding and checksum.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(CURSOR_FORMAT);
        bytes.extend_from_slice(self.observation.as_bytes());
        bytes.extend_from_slice(&self.offset.to_be_bytes());
        append_checksum(&mut bytes);
        bytes
    }
    /// Decodes a persisted cursor. The retained reader checks its observation and bounds.
    ///
    /// # Errors
    /// Rejects unknown versions, incomplete records, changed checksums, or trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        let payload = checked_record(bytes, CURSOR_FORMAT)?;
        let mut reader = CanonicalReader::new(&payload[8..], CodecLimits::PRODUCTION);
        let observation = Sha256Digest::new(reader.read_fixed().map_err(|_| record_error())?);
        let offset = reader.read_u64().map_err(|_| record_error())?;
        reader.finish().map_err(|_| record_error())?;
        Ok(Self { observation, offset })
    }
}

fn append_checksum(bytes: &mut Vec<u8>) {
    let digest = sha256(bytes);
    bytes.extend_from_slice(digest.as_bytes());
}
fn checked_record<'a>(bytes: &'a [u8], version: &[u8]) -> Result<&'a [u8], WorkspaceError> {
    let end = bytes.len().checked_sub(32).ok_or_else(record_error)?;
    let payload = bytes.get(..end).ok_or_else(record_error)?;
    if !payload.starts_with(version) || sha256(payload).as_bytes() != &bytes[end..] {
        return Err(record_error());
    }
    Ok(payload)
}
const fn record_error() -> WorkspaceError {
    invalid("invalid retained inspection metadata or cursor")
}
