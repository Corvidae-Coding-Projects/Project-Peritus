//! Immutable native-host identity supplied by the release runner.

use peritus_types::Sha256Digest;

use crate::digest_bytes;

/// Digest of reviewed native host-image and executor-policy facts.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HostFingerprint(Sha256Digest);

impl HostFingerprint {
    /// Binds the exact reviewed host-fact document used by a campaign.
    #[must_use]
    pub fn from_document(document: &[u8]) -> Self {
        Self(digest_bytes(document))
    }

    pub(super) const fn digest(self) -> Sha256Digest {
        self.0
    }
}
