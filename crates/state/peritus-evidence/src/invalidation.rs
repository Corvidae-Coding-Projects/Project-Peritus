//! Explicit immutable evidence invalidations.

use crate::canonical::{put_digest, put_u64};
use crate::{EvidenceError, EvidenceErrorKind, EvidenceId, RecoveryAction};
use peritus_codec::sha256;
use peritus_types::{EventId, Sha256Digest};

/// Immutable explicit invalidation tied to a later committed journal event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceInvalidation {
    target: EvidenceId,
    global_position: u64,
    event_id: EventId,
    event_hash: Sha256Digest,
    reason_digest: Sha256Digest,
    invalidation_digest: Sha256Digest,
}

impl EvidenceInvalidation {
    /// Creates a structurally checked invalidation request.
    ///
    /// # Errors
    ///
    /// Rejects position zero. Durable application additionally proves the event and ordering.
    pub fn new(
        target: EvidenceId,
        global_position: u64,
        event_id: EventId,
        event_hash: Sha256Digest,
        reason_digest: Sha256Digest,
    ) -> Result<Self, EvidenceError> {
        if global_position == 0 {
            return Err(EvidenceError::new(
                EvidenceErrorKind::InvalidInput,
                RecoveryAction::CorrectInput,
                "validate evidence invalidation",
                "journal position must be positive",
            ));
        }
        let mut bytes = b"peritus-evidence-invalidation-v1\0".to_vec();
        bytes.extend_from_slice(target.as_bytes());
        put_u64(&mut bytes, global_position);
        bytes.extend_from_slice(event_id.as_bytes());
        put_digest(&mut bytes, event_hash);
        put_digest(&mut bytes, reason_digest);
        Ok(Self {
            target,
            global_position,
            event_id,
            event_hash,
            reason_digest,
            invalidation_digest: sha256(&bytes),
        })
    }
    /// Returns the invalidated evidence identity.
    #[must_use]
    pub const fn target(self) -> EvidenceId {
        self.target
    }
    /// Returns the later invalidating journal position.
    #[must_use]
    pub const fn global_position(self) -> u64 {
        self.global_position
    }
    /// Returns the invalidating event identity.
    #[must_use]
    pub const fn event_id(self) -> EventId {
        self.event_id
    }
    /// Returns the invalidating event hash.
    #[must_use]
    pub const fn event_hash(self) -> Sha256Digest {
        self.event_hash
    }
    /// Returns the opaque reason digest.
    #[must_use]
    pub const fn reason_digest(self) -> Sha256Digest {
        self.reason_digest
    }
    /// Returns the digest over every invalidation field.
    #[must_use]
    pub const fn invalidation_digest(self) -> Sha256Digest {
        self.invalidation_digest
    }
}
