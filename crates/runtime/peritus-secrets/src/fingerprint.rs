//! Keyed domain-separated redaction fingerprints.

use core::fmt;

use peritus_types::Sha256Digest;

use crate::{SecretError, SecretMaterial};

/// One nonreversible keyed redaction fingerprint.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct RedactionFingerprint {
    digest: Sha256Digest,
    fragment_length: u32,
}

impl RedactionFingerprint {
    /// Returns the non-secret fingerprint digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    /// Returns the candidate fragment length.
    #[must_use]
    pub const fn fragment_length(self) -> u32 {
        self.fragment_length
    }
}

impl fmt::Debug for RedactionFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedactionFingerprint")
            .field("digest", &self.digest)
            .field("fragment_length", &self.fragment_length)
            .finish()
    }
}

/// Expiring exact-value and bounded-fragment fingerprint set.
pub struct RedactionSet {
    key: [u8; 32],
    fingerprints: Vec<RedactionFingerprint>,
    expires_epoch_millis: u64,
}

impl RedactionSet {
    /// Builds keyed fingerprints for the exact value and unique fragments from 4 through 64 bytes.
    ///
    /// # Errors
    /// Rejects a zero expiry or excessive fingerprint count.
    pub fn new(
        material: &SecretMaterial,
        key: [u8; 32],
        expires_epoch_millis: u64,
    ) -> Result<Self, SecretError> {
        if expires_epoch_millis == 0 {
            return Err(crate::error::invalid("redaction fingerprint expiry is zero"));
        }
        let mut fingerprints = material.expose(|bytes| {
            let mut values = Vec::new();
            values.push(fingerprint(&key, b"exact", bytes));
            let maximum = bytes.len().min(64);
            for length in 4..=maximum {
                for fragment in bytes.windows(length) {
                    values.push(fingerprint(&key, b"fragment", fragment));
                }
            }
            values
        });
        fingerprints.sort_by_key(|value| (value.fragment_length, value.digest));
        fingerprints.dedup();
        if fingerprints.len() > 65_536 {
            return Err(crate::error::invalid("redaction fingerprint set exceeds its bound"));
        }
        Ok(Self { key, fingerprints, expires_epoch_millis })
    }

    /// Tests whether candidate bytes equal a retained exact value or fragment.
    #[must_use]
    pub fn matches(&self, candidate: &[u8], now_epoch_millis: u64) -> bool {
        if now_epoch_millis >= self.expires_epoch_millis {
            return false;
        }
        let exact = fingerprint(&self.key, b"exact", candidate);
        let fragment = fingerprint(&self.key, b"fragment", candidate);
        self.fingerprints.iter().any(|expected| {
            constant_time_digest(expected.digest, exact.digest)
                || constant_time_digest(expected.digest, fragment.digest)
        })
    }

    /// Returns retained non-secret fingerprints.
    #[must_use]
    pub fn fingerprints(&self) -> &[RedactionFingerprint] {
        &self.fingerprints
    }
}

impl fmt::Debug for RedactionSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedactionSet")
            .field("key", &"[REDACTED]")
            .field("fingerprints", &self.fingerprints.len())
            .field("expires_epoch_millis", &self.expires_epoch_millis)
            .finish()
    }
}

impl Drop for RedactionSet {
    fn drop(&mut self) {
        self.key.fill(0);
    }
}

fn fingerprint(key: &[u8; 32], domain: &[u8], bytes: &[u8]) -> RedactionFingerprint {
    let mut input = Vec::with_capacity(32 + domain.len() + bytes.len() + 8);
    input.extend_from_slice(b"PERITUS_REDACTION\0\x01");
    input.extend_from_slice(key);
    input.extend_from_slice(domain);
    input.extend_from_slice(bytes);
    RedactionFingerprint {
        digest: peritus_codec::sha256(&input),
        fragment_length: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
    }
}

fn constant_time_digest(left: Sha256Digest, right: Sha256Digest) -> bool {
    left.as_bytes()
        .iter()
        .zip(right.as_bytes())
        .fold(0_u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}
