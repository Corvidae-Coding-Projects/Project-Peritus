//! Durable authority-clock and credential-registry values.

mod registry_commit;

use crate::{ExactFrame, JournalError, JournalErrorKind};
use peritus_approval::CredentialRegistrySnapshot;
use peritus_codec::{CodecLimits, decode_frame, encode_frame, sha256};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// C0-private B3 family used for canonical credential-registry snapshot payloads.
pub const CREDENTIAL_REGISTRY_FRAME_FAMILY: u16 = 49_153;
/// Schema version of the canonical credential-registry snapshot payload.
pub const CREDENTIAL_REGISTRY_FRAME_SCHEMA: u16 = 1;

/// Positive, monotonically allocated authority-clock epoch.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AuthorityEpoch(u64);

impl AuthorityEpoch {
    /// Returns the mathematical positive epoch value used by specifications.
    pub closed spec fn spec_value(&self) -> int {
        self.0 as int
    }

    /// Returns the primitive positive value.
    #[must_use]
    pub const fn get(self) -> (value: u64)
        ensures value == self.spec_value()
    {
        self.0
    }

    const fn try_new(value: u64) -> (result: Option<Self>)
        ensures result.is_some() == (value > 0)
    {
        if value == 0 { None } else { Some(Self(value)) }
    }

    const fn try_next(self) -> (result: Option<Self>)
        ensures
            match result {
                Some(next) => next.spec_value() == self.spec_value() + 1,
                None => self.spec_value() == u64::MAX,
            }
    {
        match self.0.checked_add(1) {
            Some(next) => Some(Self(next)),
            None => None,
        }
    }
}

} // verus!

impl AuthorityEpoch {
    /// Creates a positive authority epoch.
    ///
    /// # Errors
    ///
    /// Rejects zero, which is reserved for the absent clock state.
    pub const fn new(value: u64) -> Result<Self, JournalError> {
        match Self::try_new(value) {
            Some(epoch) => Ok(epoch),
            None => Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "validate authority epoch",
                "authority epoch must be positive",
            )),
        }
    }

    /// Advances exactly once without wraparound.
    ///
    /// # Errors
    ///
    /// Returns sequence overflow at [`u64::MAX`].
    pub const fn checked_next(self) -> Result<Self, JournalError> {
        match self.try_next() {
            Some(next) => Ok(next),
            None => Err(JournalError::new(
                JournalErrorKind::SequenceOverflow,
                "allocate authority epoch",
                "authority epoch exhausted",
            )),
        }
    }
}

/// Exact authority-clock compare-and-swap expectation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExpectedAuthorityEpoch {
    /// The clock has not allocated its first epoch.
    Absent,
    /// The clock must equal this exact current epoch.
    Current(AuthorityEpoch),
}

/// Opaque post-commit observation of one successfully allocated durable authority epoch.
///
/// The inner [`AuthorityEpoch`] remains a logical value. Only the journal adapter can construct
/// this move-only observation after its compare-and-swap transaction commits.
#[derive(Debug, Eq, PartialEq)]
pub struct AllocatedAuthorityEpoch {
    pub(crate) epoch: AuthorityEpoch,
}

impl AllocatedAuthorityEpoch {
    /// Returns the exact allocated epoch.
    #[must_use]
    pub const fn epoch(&self) -> AuthorityEpoch {
        self.epoch
    }

    /// Returns the positive primitive epoch value.
    #[must_use]
    pub const fn get(&self) -> u64 {
        self.epoch.get()
    }
}

/// Opaque current read observation of the durable authority-clock row.
#[derive(Debug, Eq, PartialEq)]
pub struct CurrentAuthorityEpoch {
    pub(crate) epoch: AuthorityEpoch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistryExpectation {
    pub(crate) revision: u64,
    pub(crate) generation: u64,
    pub(crate) digest: Sha256Digest,
}

impl CurrentAuthorityEpoch {
    /// Returns the exact current epoch.
    #[must_use]
    pub const fn epoch(&self) -> AuthorityEpoch {
        self.epoch
    }

    /// Returns the positive primitive epoch value.
    #[must_use]
    pub const fn get(&self) -> u64 {
        self.epoch.get()
    }
}

/// Exact checked credential-registry row to install atomically.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialRegistryInstall {
    expected_revision: Option<u64>,
    revision: u64,
    generation: u64,
    snapshot: ExactFrame,
    digest: Sha256Digest,
}

impl CredentialRegistryInstall {
    /// Encodes one checked B1 snapshot for an exact C0 registry installation.
    ///
    /// # Errors
    ///
    /// Rejects a zero generation, a revision that is not the exact successor of its precondition,
    /// an over-limit canonical snapshot, or a snapshot that cannot fit the production B3 frame.
    pub fn new(
        expected_revision: Option<u64>,
        generation: u64,
        snapshot: &CredentialRegistrySnapshot,
    ) -> Result<Self, JournalError> {
        let revision = snapshot.revision().get();
        let valid_revision = crate::verified::cas_successor(expected_revision, revision);
        if !valid_revision || generation == 0 {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "validate credential registry install",
                "registry revision or generation is not positive and monotonic",
            ));
        }
        let payload = snapshot.canonical_bytes().map_err(|_| {
            JournalError::new(
                JournalErrorKind::InvalidInput,
                "encode credential registry snapshot",
                "checked registry snapshot cannot be represented canonically",
            )
        })?;
        let digest = snapshot.digest().map_err(|_| {
            JournalError::new(
                JournalErrorKind::InvalidInput,
                "digest credential registry snapshot",
                "checked registry snapshot cannot be hashed canonically",
            )
        })?;
        let frame = encode_frame(
            CREDENTIAL_REGISTRY_FRAME_FAMILY,
            CREDENTIAL_REGISTRY_FRAME_SCHEMA,
            &payload,
            CodecLimits::PRODUCTION,
        )
        .map_err(|_| {
            JournalError::new(
                JournalErrorKind::InvalidInput,
                "frame credential registry snapshot",
                "canonical registry snapshot exceeds the production frame limits",
            )
        })?;
        let snapshot = ExactFrame::new(frame)?;
        if credential_registry_payload_digest(&snapshot)? != digest {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "bind credential registry snapshot",
                "framed payload differs from the checked registry snapshot digest",
            ));
        }
        Ok(Self { expected_revision, revision, generation, snapshot, digest })
    }

    /// Returns the required previous revision, or absence.
    #[must_use]
    pub const fn expected_revision(&self) -> Option<u64> {
        self.expected_revision
    }

    /// Returns the installed revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the current same-key lineage generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Borrows exact complete snapshot-frame bytes.
    #[must_use]
    pub fn snapshot_bytes(&self) -> &[u8] {
        self.snapshot.bytes()
    }

    /// Returns the digest of the exact canonical checked snapshot payload.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Validates the private registry frame schema and hashes its exact canonical payload.
///
/// # Errors
///
/// Rejects a frame with the wrong registry family/schema or invalid canonical framing.
pub fn credential_registry_payload_digest(
    snapshot: &ExactFrame,
) -> Result<Sha256Digest, JournalError> {
    Ok(sha256(credential_registry_payload(snapshot)?))
}

pub(super) fn credential_registry_payload(snapshot: &ExactFrame) -> Result<&[u8], JournalError> {
    if snapshot.family() != CREDENTIAL_REGISTRY_FRAME_FAMILY
        || snapshot.schema_version() != CREDENTIAL_REGISTRY_FRAME_SCHEMA
    {
        return Err(JournalError::new(
            JournalErrorKind::InvalidInput,
            "validate credential registry snapshot",
            "credential registry snapshot uses an unsupported schema",
        ));
    }
    let frame = decode_frame(snapshot.bytes(), CodecLimits::PRODUCTION).map_err(|_| {
        JournalError::new(
            JournalErrorKind::InvalidInput,
            "validate credential registry snapshot",
            "credential registry snapshot frame is not canonical",
        )
    })?;
    Ok(frame.payload())
}
