//! Fixed-size activation acknowledgement bound to admitted native state.

use peritus_types::Sha256Digest;

/// Exact activation record size emitted after native controls are installed.
pub const ACTIVATION_RECORD_BYTES: usize = Sha256Digest::LENGTH;

/// Fixed-size helper activation acknowledgment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivationRecord {
    digest: Sha256Digest,
}

impl ActivationRecord {
    /// Creates successful activation evidence bound to the exact manifest and preparation.
    #[must_use]
    pub fn new(manifest_digest: Sha256Digest, preparation_digest: Sha256Digest) -> Self {
        Self {
            digest: peritus_process::native_activation_record(manifest_digest, preparation_digest),
        }
    }

    /// Encodes the exact fixed-size record.
    #[must_use]
    pub const fn encode(self) -> [u8; ACTIVATION_RECORD_BYTES] {
        self.digest.into_bytes()
    }

    /// Decodes one exact fixed-size record. Call [`Self::matches`] with the expected binding.
    #[must_use]
    pub const fn decode(bytes: [u8; ACTIVATION_RECORD_BYTES]) -> Self {
        Self { digest: Sha256Digest::new(bytes) }
    }

    /// Returns the protocol digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }

    /// Checks the record against the exact expected manifest and preparation identities.
    #[must_use]
    pub fn matches(self, manifest_digest: Sha256Digest, preparation_digest: Sha256Digest) -> bool {
        self.digest
            == peritus_process::native_activation_record(manifest_digest, preparation_digest)
    }
}
