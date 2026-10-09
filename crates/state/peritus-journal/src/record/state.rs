//! Journal-owned durable state values and compare-and-swap installs.

use peritus_codec::sha256;
use peritus_types::Sha256Digest;

use crate::{JournalError, JournalErrorKind};

/// Maximum opaque bytes in one journal-owned durable state value.
pub const MAX_STATE_BYTES: usize = 16 * 1024 * 1024;
/// Maximum bytes in one state-record key.
pub const MAX_STATE_KEY_BYTES: usize = 1_024;

/// Checked exact state record installed atomically with an event batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateInstall {
    namespace: u16,
    key: Vec<u8>,
    expected_revision: Option<u64>,
    revision: u64,
    bytes: Vec<u8>,
    digest: Sha256Digest,
}

/// Exact current observation of one journal-owned durable state row.
#[derive(Debug, Eq, PartialEq)]
pub struct DurableStateRecord {
    pub(crate) namespace: u16,
    pub(crate) key: Vec<u8>,
    pub(crate) revision: u64,
    pub(crate) bytes: Vec<u8>,
    pub(crate) digest: Sha256Digest,
    pub(crate) producing_position: u64,
}

impl DurableStateRecord {
    /// Returns the nonzero state namespace.
    #[must_use]
    pub const fn namespace(&self) -> u16 {
        self.namespace
    }

    /// Borrows the exact binary state key.
    #[must_use]
    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// Returns the positive compare-and-swap revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Borrows the exact stored state bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the verified digest of the exact bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns the event position that installed this revision.
    #[must_use]
    pub const fn producing_position(&self) -> u64 {
        self.producing_position
    }
}

impl StateInstall {
    /// Validates a state-record CAS and exact opaque payload.
    ///
    /// # Errors
    ///
    /// Rejects reserved namespaces, empty or oversized keys, oversized payloads, zero revisions,
    /// and non-successor CAS revisions.
    pub fn new(
        namespace: u16,
        key: Vec<u8>,
        expected_revision: Option<u64>,
        revision: u64,
        bytes: Vec<u8>,
    ) -> Result<Self, JournalError> {
        let valid_revision = crate::verified::cas_successor(expected_revision, revision);
        if namespace == 0
            || key.is_empty()
            || key.len() > MAX_STATE_KEY_BYTES
            || bytes.len() > MAX_STATE_BYTES
            || !valid_revision
        {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "validate state install",
                "invalid namespace, bound, or successor revision",
            ));
        }
        let digest = sha256(&bytes);
        Ok(Self { namespace, key, expected_revision, revision, bytes, digest })
    }

    /// Returns the nonzero state namespace.
    #[must_use]
    pub const fn namespace(&self) -> u16 {
        self.namespace
    }

    /// Borrows the bounded binary key.
    #[must_use]
    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// Returns the expected prior revision, or absence.
    #[must_use]
    pub const fn expected_revision(&self) -> Option<u64> {
        self.expected_revision
    }

    /// Returns the new positive revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Borrows exact state bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns SHA-256 over the exact state bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}
