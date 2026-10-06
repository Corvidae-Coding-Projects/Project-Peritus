//! Owner-persisted listing metadata and resumable positions, without effect authority.

use super::super::{WorkspaceError, invalid};
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;

/// Exact retained directory observation. The owner publishes this with its backing file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedDirectory {
    pub(super) folder: Sha256Digest,
    pub(super) path: Option<WorkspacePath>,
    pub(super) count: u64,
    pub(super) bytes: u64,
    pub(super) digest: Sha256Digest,
}
impl ObservedDirectory {
    /// Returns the opened source folder identity, not effect authority.
    #[must_use]
    pub const fn folder(&self) -> Sha256Digest {
        self.folder
    }
    /// Returns the selected directory, or the opened root.
    #[must_use]
    pub const fn path(&self) -> Option<&WorkspacePath> {
        self.path.as_ref()
    }
    /// Returns the number of exposed native children, including exclusions.
    #[must_use]
    pub const fn count(&self) -> u64 {
        self.count
    }
    /// Returns the exact retained body identity.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Encodes versioned metadata for owner-managed durable publication.
    #[must_use]
    pub fn to_record(&self) -> Vec<u8> {
        let mut bytes = b"PDOBv001".to_vec();
        bytes.push(platform_tag());
        bytes.extend_from_slice(self.folder.as_bytes());
        match &self.path {
            None => bytes.push(0),
            Some(path) => {
                bytes.push(1);
                bytes.extend_from_slice(&(path.as_str().len() as u64).to_be_bytes());
                bytes.extend_from_slice(path.as_str().as_bytes());
            }
        }
        bytes.extend_from_slice(&self.count.to_be_bytes());
        bytes.extend_from_slice(&self.bytes.to_be_bytes());
        bytes.extend_from_slice(self.digest.as_bytes());
        seal(bytes)
    }
    /// Decodes checksummed metadata. A matching backing body must still be verified on open.
    ///
    /// # Errors
    /// Rejects unknown versions, foreign platforms, malformed paths/counts and corruption.
    pub fn from_record(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        let mut payload = checked(bytes, *b"PDOBv001")?;
        if take::<1>(&mut payload)?[0] != platform_tag() {
            return Err(invalid("directory observation belongs to another platform"));
        }
        let folder = Sha256Digest::new(take::<32>(&mut payload)?);
        let path = match take::<1>(&mut payload)?[0] {
            0 => None,
            1 => {
                let length = usize::try_from(u64::from_be_bytes(take::<8>(&mut payload)?))
                    .map_err(|_| invalid("directory path length is not representable"))?;
                let text =
                    payload.get(..length).ok_or_else(|| invalid("directory path is incomplete"))?;
                let path = WorkspacePath::new(
                    std::str::from_utf8(text)
                        .map_err(|_| invalid("directory path is not UTF-8"))?,
                )
                .map_err(|_| invalid("directory path is unsafe"))?;
                payload = &payload[length..];
                Some(path)
            }
            _ => return Err(invalid("unknown directory path representation")),
        };
        let count = u64::from_be_bytes(take::<8>(&mut payload)?);
        let bytes = u64::from_be_bytes(take::<8>(&mut payload)?);
        let digest = Sha256Digest::new(take::<32>(&mut payload)?);
        if !payload.is_empty() || bytes < 9 || count > (bytes - 9) / 27 {
            return Err(invalid("invalid directory observation cardinality"));
        }
        Ok(Self { folder, path, count, bytes, digest })
    }
    pub(super) fn binding(&self) -> Sha256Digest {
        peritus_codec::sha256(&self.to_record())
    }
}

/// Position in one exact retained listing. Resume verifies the record boundary and ordinal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryCursor {
    pub(super) binding: Sha256Digest,
    pub(super) index: u64,
    pub(super) offset: u64,
}
impl DirectoryCursor {
    /// Returns the zero-based next native child ordinal.
    #[must_use]
    pub const fn index(self) -> u64 {
        self.index
    }
    /// Encodes a checksummed continuation for durable owner storage.
    #[must_use]
    pub fn to_record(self) -> Vec<u8> {
        let mut bytes = b"PDCUv001".to_vec();
        bytes.extend_from_slice(self.binding.as_bytes());
        bytes.extend_from_slice(&self.index.to_be_bytes());
        bytes.extend_from_slice(&self.offset.to_be_bytes());
        seal(bytes)
    }
    /// Decodes a continuation. Opening the retained reader validates its exact boundary.
    ///
    /// # Errors
    /// Rejects unknown versions, malformed/trailing data and corruption.
    pub fn from_record(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        let mut payload = checked(bytes, *b"PDCUv001")?;
        let binding = Sha256Digest::new(take::<32>(&mut payload)?);
        let index = u64::from_be_bytes(take::<8>(&mut payload)?);
        let offset = u64::from_be_bytes(take::<8>(&mut payload)?);
        if !payload.is_empty() {
            return Err(invalid("directory cursor has trailing data"));
        }
        Ok(Self { binding, index, offset })
    }
}

pub(super) const fn platform_tag() -> u8 {
    super::super::NATIVE_PLATFORM_TAG
}
fn seal(mut bytes: Vec<u8>) -> Vec<u8> {
    let digest = peritus_codec::sha256(&bytes);
    bytes.extend_from_slice(digest.as_bytes());
    bytes
}
fn checked(bytes: &[u8], magic: [u8; 8]) -> Result<&[u8], WorkspaceError> {
    let length =
        bytes.len().checked_sub(32).ok_or_else(|| invalid("directory record is incomplete"))?;
    let (payload, checksum) = bytes.split_at(length);
    if !payload.starts_with(&magic) || peritus_codec::sha256(payload).as_bytes() != checksum {
        return Err(invalid("directory record version or checksum is invalid"));
    }
    Ok(&payload[magic.len()..])
}
fn take<const N: usize>(bytes: &mut &[u8]) -> Result<[u8; N], WorkspaceError> {
    let value = bytes
        .get(..N)
        .ok_or_else(|| invalid("directory record is incomplete"))?
        .try_into()
        .map_err(|_| invalid("directory field is incomplete"))?;
    *bytes = &bytes[N..];
    Ok(value)
}
