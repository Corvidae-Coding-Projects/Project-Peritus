//! Checked application-principal values.

use peritus_types::{ActorId, Sha256Digest};

/// Operating-system principal namespace.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationPrincipalKind {
    /// Unix-domain peer credentials.
    UnixPeer,
    /// Windows named-pipe peer credentials.
    WindowsPeer,
    /// Locally provisioned service identity.
    Service,
}

impl ApplicationPrincipalKind {
    pub(super) const fn tag(self) -> i64 {
        match self {
            Self::UnixPeer => 1,
            Self::WindowsPeer => 2,
            Self::Service => 3,
        }
    }

    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::UnixPeer),
            2 => Some(Self::WindowsPeer),
            3 => Some(Self::Service),
            _ => None,
        }
    }
}

/// Durable principal binding state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationPrincipalState {
    /// The binding may authenticate new connections.
    Active,
    /// The binding has been explicitly revoked.
    Revoked,
}

impl ApplicationPrincipalState {
    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::Active),
            2 => Some(Self::Revoked),
            _ => None,
        }
    }
}

/// New immutable OS-principal to actor binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewApplicationPrincipal {
    pub(super) principal_digest: Sha256Digest,
    pub(super) kind: ApplicationPrincipalKind,
    pub(super) actor_id: ActorId,
    pub(super) binding_digest: Sha256Digest,
}

impl NewApplicationPrincipal {
    /// Creates a checked principal binding request.
    #[must_use]
    pub const fn new(
        principal_digest: Sha256Digest,
        kind: ApplicationPrincipalKind,
        actor_id: ActorId,
        binding_digest: Sha256Digest,
    ) -> Self {
        Self { principal_digest, kind, actor_id, binding_digest }
    }
}

/// One durable OS-principal to human actor binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationPrincipal {
    pub(super) principal_digest: Sha256Digest,
    pub(super) kind: ApplicationPrincipalKind,
    pub(super) actor_id: ActorId,
    pub(super) binding_digest: Sha256Digest,
    pub(super) state: ApplicationPrincipalState,
}

impl ApplicationPrincipal {
    /// Returns the canonical principal digest.
    #[must_use]
    pub const fn principal_digest(&self) -> Sha256Digest {
        self.principal_digest
    }

    /// Returns the peer namespace.
    #[must_use]
    pub const fn kind(&self) -> ApplicationPrincipalKind {
        self.kind
    }

    /// Returns the uniquely bound human actor.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }

    /// Returns the provisioned binding digest.
    #[must_use]
    pub const fn binding_digest(&self) -> Sha256Digest {
        self.binding_digest
    }

    /// Returns current binding state.
    #[must_use]
    pub const fn state(&self) -> ApplicationPrincipalState {
        self.state
    }
}
