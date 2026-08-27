//! Canonical authenticated operating-system peer identity.

use peritus_journal::ApplicationPrincipalKind;
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

/// Authenticated local operating-system principal facts.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PeerIdentity {
    kind: ApplicationPrincipalKind,
    principal_digest: Sha256Digest,
}

impl PeerIdentity {
    pub(super) fn from_os_identity(kind: ApplicationPrincipalKind, identity: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"peritus/local-peer/v1\0");
        hasher.update([match kind {
            ApplicationPrincipalKind::UnixPeer => 1,
            ApplicationPrincipalKind::WindowsPeer => 2,
            ApplicationPrincipalKind::Service => 3,
        }]);
        hasher.update((identity.len() as u64).to_be_bytes());
        hasher.update(identity);
        Self { kind, principal_digest: Sha256Digest::new(hasher.finalize().into()) }
    }

    /// Returns the authenticated platform principal namespace.
    #[must_use]
    pub const fn kind(self) -> ApplicationPrincipalKind {
        self.kind
    }
    /// Returns the domain-separated digest of exact OS identity bytes.
    #[must_use]
    pub const fn principal_digest(self) -> Sha256Digest {
        self.principal_digest
    }
}
