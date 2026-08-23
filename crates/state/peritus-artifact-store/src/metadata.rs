//! Durable artifact metadata value types.

use peritus_types::{EventId, Sha256Digest};

use crate::{ArtifactDigest, ArtifactStoreError, ErrorCode, RecoveryClass};

const MAX_MEDIA_TYPE_BYTES: usize = 255;
const MAX_ENCRYPTION_ALGORITHM_BYTES: usize = 64;

/// Validated Internet media type text.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MediaType(String);

impl MediaType {
    /// Validates and stores an ASCII media type such as `application/json`.
    ///
    /// Parameters are allowed, but control characters, whitespace, and missing type/subtype
    /// separators are rejected.
    ///
    /// # Errors
    ///
    /// Returns an error when the value is empty, too long, non-ASCII, or structurally invalid.
    pub fn new(value: impl Into<String>) -> Result<Self, ArtifactStoreError> {
        let value = value.into();
        let bytes = value.as_bytes();
        let valid_length = !bytes.is_empty() && bytes.len() <= MAX_MEDIA_TYPE_BYTES;
        let valid_bytes = bytes.iter().all(u8::is_ascii_graphic);
        let essence = value.split(';').next().unwrap_or_default();
        let valid_essence = essence
            .split_once('/')
            .is_some_and(|(kind, subtype)| !kind.is_empty() && !subtype.is_empty());
        if !valid_length || !valid_bytes || !valid_essence {
            return Err(invalid_metadata("invalid artifact media type"));
        }
        Ok(Self(value))
    }

    /// Returns the exact validated text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Encryption metadata recorded alongside artifact identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptionMetadata {
    kind: EncryptionKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum EncryptionKind {
    Unencrypted,
    Envelope { algorithm: String, key_reference: Sha256Digest, parameters_digest: Sha256Digest },
}

impl EncryptionMetadata {
    /// Describes plaintext artifact bytes.
    #[must_use]
    pub const fn unencrypted() -> Self {
        Self { kind: EncryptionKind::Unencrypted }
    }

    /// Describes bytes encrypted by an external envelope/key service.
    ///
    /// The artifact store records binding metadata only and never obtains key material.
    ///
    /// # Errors
    ///
    /// Returns an error when the algorithm token is empty, too long, or contains non-token bytes.
    pub fn envelope(
        algorithm: impl Into<String>,
        key_reference: Sha256Digest,
        parameters_digest: Sha256Digest,
    ) -> Result<Self, ArtifactStoreError> {
        let algorithm = algorithm.into();
        let valid = !algorithm.is_empty()
            && algorithm.len() <= MAX_ENCRYPTION_ALGORITHM_BYTES
            && algorithm.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/')
            });
        if !valid {
            return Err(invalid_metadata("invalid encryption algorithm token"));
        }
        Ok(Self { kind: EncryptionKind::Envelope { algorithm, key_reference, parameters_digest } })
    }

    /// Returns whether the artifact bytes are encrypted.
    #[must_use]
    pub const fn is_encrypted(&self) -> bool {
        matches!(self.kind, EncryptionKind::Envelope { .. })
    }

    /// Returns the envelope algorithm, when encrypted.
    #[must_use]
    pub fn algorithm(&self) -> Option<&str> {
        match &self.kind {
            EncryptionKind::Unencrypted => None,
            EncryptionKind::Envelope { algorithm, .. } => Some(algorithm),
        }
    }

    /// Returns the opaque key reference digest, when encrypted.
    #[must_use]
    pub const fn key_reference(&self) -> Option<Sha256Digest> {
        match self.kind {
            EncryptionKind::Unencrypted => None,
            EncryptionKind::Envelope { key_reference, .. } => Some(key_reference),
        }
    }

    /// Returns the digest of canonical encryption parameters, when encrypted.
    #[must_use]
    pub const fn parameters_digest(&self) -> Option<Sha256Digest> {
        match self.kind {
            EncryptionKind::Unencrypted => None,
            EncryptionKind::Envelope { parameters_digest, .. } => Some(parameters_digest),
        }
    }
}

/// Publication state tracked by durable metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalizationState {
    /// Bytes are being streamed and are not referenceable.
    Partial,
    /// Exact bytes have been synchronized and published.
    Finalized,
}

/// Collection state tracked by durable metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuarantineState {
    /// The object is in the active object namespace.
    Active,
    /// The object moved to quarantine during the named generation.
    Quarantined {
        /// Generation whose applied plan quarantined the artifact.
        since: crate::CollectionGeneration,
    },
}

/// Complete metadata value persisted by the C0 database boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactMetadata {
    digest: ArtifactDigest,
    size: u64,
    media_type: MediaType,
    encryption: EncryptionMetadata,
    finalization: FinalizationState,
    creating_event: EventId,
    quarantine: QuarantineState,
}

impl ArtifactMetadata {
    /// Creates metadata for an artifact record.
    #[must_use]
    pub const fn new(
        digest: ArtifactDigest,
        size: u64,
        media_type: MediaType,
        encryption: EncryptionMetadata,
        finalization: FinalizationState,
        creating_event: EventId,
        quarantine: QuarantineState,
    ) -> Self {
        Self { digest, size, media_type, encryption, finalization, creating_event, quarantine }
    }

    /// Returns the content digest.
    #[must_use]
    pub const fn digest(&self) -> ArtifactDigest {
        self.digest
    }

    /// Returns the exact byte size.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// Returns the media type.
    #[must_use]
    pub const fn media_type(&self) -> &MediaType {
        &self.media_type
    }

    /// Returns encryption binding metadata.
    #[must_use]
    pub const fn encryption(&self) -> &EncryptionMetadata {
        &self.encryption
    }

    /// Returns finalization state.
    #[must_use]
    pub const fn finalization(&self) -> FinalizationState {
        self.finalization
    }

    /// Returns the creating journal event.
    #[must_use]
    pub const fn creating_event(&self) -> EventId {
        self.creating_event
    }

    /// Returns collection state.
    #[must_use]
    pub const fn quarantine(&self) -> QuarantineState {
        self.quarantine
    }

    /// Returns whether a journal transaction may reference this metadata record.
    #[must_use]
    pub const fn is_referenceable(&self) -> bool {
        matches!(self.finalization, FinalizationState::Finalized)
            && matches!(self.quarantine, QuarantineState::Active)
    }
}

const fn invalid_metadata(message: &'static str) -> ArtifactStoreError {
    ArtifactStoreError::message(ErrorCode::InvalidMetadata, RecoveryClass::CorrectRequest, message)
}
