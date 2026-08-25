//! Sensitive-value consumption and encrypted artifact-vault references.

use core::fmt;

use peritus_artifact_store::{
    ArtifactDigest, ArtifactMetadata, FinalizationState, QuarantineState,
};
use peritus_types::{EventId, Sha256Digest};
use zeroize::Zeroizing;

use crate::{TraceError, TraceErrorKind};

/// Closed sensitive-content classification.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SensitivityClass {
    /// User or system prompt content.
    Prompt,
    /// Model-generated output content.
    ModelOutput,
    /// Tool-call argument content.
    ToolArguments,
    /// Secret material.
    Secret,
    /// Credential material or credential-bearing headers.
    Credential,
    /// Environment variable names or values.
    Environment,
    /// Workspace file content or paths supplied as content.
    WorkspaceContent,
}

impl SensitivityClass {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Prompt => 1,
            Self::ModelOutput => 2,
            Self::ToolArguments => 3,
            Self::Secret => 4,
            Self::Credential => 5,
            Self::Environment => 6,
            Self::WorkspaceContent => 7,
        }
    }

    pub(crate) const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Prompt),
            2 => Some(Self::ModelOutput),
            3 => Some(Self::ToolArguments),
            4 => Some(Self::Secret),
            5 => Some(Self::Credential),
            6 => Some(Self::Environment),
            7 => Some(Self::WorkspaceContent),
            _ => None,
        }
    }
}

/// Owned sensitive bytes that zeroize on drop and never reveal contents through `Debug`.
pub struct SensitivePayload {
    class: SensitivityClass,
    bytes: Zeroizing<Vec<u8>>,
}

impl SensitivePayload {
    /// Takes ownership of one bounded sensitive value.
    ///
    /// # Errors
    ///
    /// Rejects an empty value or one larger than 16 MiB.
    pub fn new(class: SensitivityClass, bytes: Vec<u8>) -> Result<Self, TraceError> {
        if bytes.is_empty() || bytes.len() > 16 * 1024 * 1024 {
            return Err(TraceError::static_error(
                TraceErrorKind::LimitExceeded,
                "accept sensitive payload",
                "sensitive payload is empty or exceeds its byte bound",
            ));
        }
        Ok(Self { class, bytes: Zeroizing::new(bytes) })
    }

    /// Returns the declared sensitivity class without exposing bytes.
    #[must_use]
    pub const fn class(&self) -> SensitivityClass {
        self.class
    }
}

impl fmt::Debug for SensitivePayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SensitivePayload")
            .field("class", &self.class)
            .field("bytes", &"[redacted]")
            .finish()
    }
}

/// Digest-verified finalized encrypted reference to authorized raw evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactVaultReference {
    digest: ArtifactDigest,
    size: u64,
    creating_event: EventId,
    key_reference: Sha256Digest,
    parameters_digest: Sha256Digest,
}

impl ArtifactVaultReference {
    fn from_metadata(metadata: &ArtifactMetadata) -> Result<Self, TraceError> {
        if metadata.finalization() != FinalizationState::Finalized
            || metadata.quarantine() != QuarantineState::Active
            || !metadata.encryption().is_encrypted()
        {
            return Err(redaction_error(
                "vault evidence must be finalized, active, and envelope encrypted",
            ));
        }
        let key_reference = metadata
            .encryption()
            .key_reference()
            .ok_or_else(|| redaction_error("vault encryption key reference is absent"))?;
        let parameters_digest = metadata
            .encryption()
            .parameters_digest()
            .ok_or_else(|| redaction_error("vault encryption parameter digest is absent"))?;
        Ok(Self {
            digest: metadata.digest(),
            size: metadata.size(),
            creating_event: metadata.creating_event(),
            key_reference,
            parameters_digest,
        })
    }

    /// Returns the exact finalized artifact digest.
    #[must_use]
    pub const fn digest(self) -> ArtifactDigest {
        self.digest
    }
    /// Returns the encrypted object size.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }
    /// Returns the creating journal event.
    #[must_use]
    pub const fn creating_event(self) -> EventId {
        self.creating_event
    }
    /// Returns the opaque envelope-key reference digest.
    #[must_use]
    pub const fn key_reference(self) -> Sha256Digest {
        self.key_reference
    }
    /// Returns the canonical encryption-parameter digest.
    #[must_use]
    pub const fn parameters_digest(self) -> Sha256Digest {
        self.parameters_digest
    }

    pub(crate) const fn from_parts(
        digest: ArtifactDigest,
        size: u64,
        creating_event: EventId,
        key_reference: Sha256Digest,
        parameters_digest: Sha256Digest,
    ) -> Self {
        Self { digest, size, creating_event, key_reference, parameters_digest }
    }
}

/// Safe result of consuming one sensitive payload.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RedactedValue {
    /// Content was discarded; only its class and byte count remain.
    Omitted {
        /// Sensitive-content class.
        class: SensitivityClass,
        /// Number of bytes consumed before omission.
        observed_bytes: u64,
    },
    /// Raw evidence is available only through this encrypted artifact reference.
    Vault {
        /// Sensitive-content class.
        class: SensitivityClass,
        /// Finalized encrypted vault reference.
        reference: ArtifactVaultReference,
    },
}

impl RedactedValue {
    /// Returns the sensitive-content class.
    #[must_use]
    pub const fn class(self) -> SensitivityClass {
        match self {
            Self::Omitted { class, .. } | Self::Vault { class, .. } => class,
        }
    }

    /// Returns the vault reference, when authorized raw evidence was supplied.
    #[must_use]
    pub const fn vault_reference(self) -> Option<ArtifactVaultReference> {
        match self {
            Self::Omitted { .. } => None,
            Self::Vault { reference, .. } => Some(reference),
        }
    }

    pub(crate) const fn omitted(class: SensitivityClass, observed_bytes: u64) -> Self {
        Self::Omitted { class, observed_bytes }
    }
}

/// Consumes and redacts sensitive bytes, optionally binding exact encrypted artifact metadata.
///
/// The artifact store is responsible for writing raw evidence before this call. C7 never offers an
/// API for reading it back. When metadata is present, the supplied bytes must match its exact digest
/// and size; otherwise no vault reference is produced.
///
/// # Errors
///
/// Returns a redaction error for unencrypted, partial, quarantined, size-mismatched, or
/// digest-mismatched metadata.
#[allow(
    clippy::needless_pass_by_value,
    reason = "redaction deliberately consumes and then zeroizes the sensitive allocation"
)]
pub fn redact_sensitive(
    payload: SensitivePayload,
    metadata: Option<&ArtifactMetadata>,
) -> Result<RedactedValue, TraceError> {
    let observed_bytes = u64::try_from(payload.bytes.len()).map_err(|_| {
        TraceError::static_error(
            TraceErrorKind::LimitExceeded,
            "redact sensitive payload",
            "sensitive payload size cannot be represented",
        )
    })?;
    let Some(metadata) = metadata else {
        return Ok(RedactedValue::Omitted { class: payload.class, observed_bytes });
    };
    let reference = ArtifactVaultReference::from_metadata(metadata)?;
    if reference.size != observed_bytes
        || reference.digest.sha256() != peritus_codec::sha256(payload.bytes.as_slice())
    {
        return Err(redaction_error("sensitive payload does not match encrypted vault metadata"));
    }
    Ok(RedactedValue::Vault { class: payload.class, reference })
}

const fn redaction_error(detail: &'static str) -> TraceError {
    TraceError::static_error(TraceErrorKind::Redaction, "redact sensitive payload", detail)
}
