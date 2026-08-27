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

#[cfg(test)]
mod tests {
    use super::{Ed25519PublicKey, Ed25519Signature, verify_detached_ed25519};
    use crate::BoundedId;

    // RFC 8032 test vector 1 authenticates the empty message. No private key is retained or used.
    const PUBLIC_KEY: [u8; 32] = [
        0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07,
        0x3a, 0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07,
        0x51, 0x1a,
    ];
    const SIGNATURE: [u8; 64] = [
        0xe5, 0x56, 0x43, 0x00, 0xc3, 0x60, 0xac, 0x72, 0x90, 0x86, 0xe2, 0xcc, 0x80, 0x6e, 0x82,
        0x8a, 0x84, 0x87, 0x7f, 0x1e, 0xb8, 0xe5, 0xd9, 0x74, 0xd8, 0x73, 0xe0, 0x65, 0x22, 0x49,
        0x01, 0x55, 0x5f, 0xb8, 0x82, 0x15, 0x90, 0xa3, 0x3b, 0xac, 0xc6, 0x1e, 0x39, 0x70, 0x1c,
        0xf9, 0xb4, 0x6b, 0xd2, 0x5b, 0xf5, 0xf0, 0x59, 0x5b, 0xbe, 0x24, 0x65, 0x51, 0x41, 0x43,
        0x8e, 0x7a, 0x10, 0x0b,
    ];

    #[test]
    fn verifies_public_rfc_vector() {
        let verified = verify_detached_ed25519(
            BoundedId::new("rfc8032-key-1").expect("key id"),
            Ed25519PublicKey::from_bytes(PUBLIC_KEY),
            Ed25519Signature::from_bytes(SIGNATURE),
            b"",
        )
        .expect("valid vector");
        assert_eq!(verified.key_id().as_str(), "rfc8032-key-1");
    }

    #[test]
    fn rejects_payload_substitution() {
        assert!(
            verify_detached_ed25519(
                BoundedId::new("rfc8032-key-1").expect("key id"),
                Ed25519PublicKey::from_bytes(PUBLIC_KEY),
                Ed25519Signature::from_bytes(SIGNATURE),
                b"substituted",
            )
            .is_err()
        );
    }
}
