//! Detached Ed25519 signature observations and verification.

use std::fmt;

use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use serde::Serialize;

use crate::{ArtifactError, ArtifactErrorCode, BoundedId, Sha256Digest, digest_bytes};

/// Exact Ed25519 public key bytes supplied by a release signer.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Ed25519PublicKey([u8; 32]);

impl Ed25519PublicKey {
    /// Creates a public-key value from exact bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns exact public-key bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Ed25519PublicKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Ed25519PublicKey").field(&encode_hex(&self.0)).finish()
    }
}

impl Serialize for Ed25519PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&encode_hex(&self.0))
    }
}

/// Exact detached Ed25519 signature bytes.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Ed25519Signature([u8; 64]);

impl Ed25519Signature {
    /// Creates a detached signature value from exact bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }

    /// Returns exact detached-signature bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

impl fmt::Debug for Ed25519Signature {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Ed25519Signature").field(&encode_hex(&self.0)).finish()
    }
}

impl Serialize for Ed25519Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&encode_hex(&self.0))
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

/// Successful detached-signature verification bound to exact payload bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VerifiedSignature {
    algorithm: &'static str,
    key_id: BoundedId,
    public_key: Ed25519PublicKey,
    signature: Ed25519Signature,
    payload_digest: Sha256Digest,
    signature_digest: Sha256Digest,
}

impl VerifiedSignature {
    /// Returns the fixed detached-signature algorithm name.
    #[must_use]
    pub const fn algorithm(&self) -> &'static str {
        self.algorithm
    }

    /// Returns the stable signer key identifier.
    #[must_use]
    pub const fn key_id(&self) -> &BoundedId {
        &self.key_id
    }

    /// Returns the verified payload digest.
    #[must_use]
    pub const fn payload_digest(&self) -> Sha256Digest {
        self.payload_digest
    }

    /// Returns the signature-content digest.
    #[must_use]
    pub const fn signature_digest(&self) -> Sha256Digest {
        self.signature_digest
    }

    /// Returns the public key used for verification.
    #[must_use]
    pub const fn public_key(&self) -> Ed25519PublicKey {
        self.public_key
    }

    /// Returns the verified detached signature.
    #[must_use]
    pub const fn signature(&self) -> Ed25519Signature {
        self.signature
    }
}

/// Verifies a detached Ed25519 signature over exact payload bytes.
///
/// This function accepts public material only. It has no key-generation or signing path.
///
/// # Errors
///
/// Returns [`ArtifactError`] when the public key is invalid or signature verification fails.
pub fn verify_detached_ed25519(
    key_id: BoundedId,
    public_key: Ed25519PublicKey,
    signature: Ed25519Signature,
    payload: &[u8],
) -> Result<VerifiedSignature, ArtifactError> {
    let verifying_key = VerifyingKey::from_bytes(public_key.as_bytes()).map_err(|_| {
        ArtifactError::new(
            ArtifactErrorCode::Signature,
            "verify detached Ed25519 signature",
            "public key is not a valid Ed25519 verifying key",
        )
    })?;
    let dalek_signature = Signature::from_bytes(signature.as_bytes());
    verifying_key.verify(payload, &dalek_signature).map_err(|_| {
        ArtifactError::new(
            ArtifactErrorCode::Signature,
            "verify detached Ed25519 signature",
            "signature does not authenticate the exact payload bytes",
        )
    })?;
    Ok(VerifiedSignature {
        algorithm: "Ed25519",
        key_id,
        public_key,
        signature,
        payload_digest: digest_bytes(payload),
        signature_digest: digest_bytes(signature.as_bytes()),
    })
}
