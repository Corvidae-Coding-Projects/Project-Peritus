//! Checked application-session values.

use peritus_types::{ActorId, SessionId};

use crate::{JournalError, JournalErrorKind};

/// Durable application session state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationSessionState {
    /// New requests may be admitted.
    Active,
    /// Existing work may drain but new mutations are rejected.
    Draining,
    /// The session is terminally closed.
    Closed,
}

impl ApplicationSessionState {
    pub(super) const fn tag(self) -> i64 {
        match self {
            Self::Active => 1,
            Self::Draining => 2,
            Self::Closed => 3,
        }
    }

    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::Active),
            2 => Some(Self::Draining),
            3 => Some(Self::Closed),
            _ => None,
        }
    }
}

/// Checked new durable session facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewApplicationSession {
    pub(super) session_id: SessionId,
    pub(super) actor_id: ActorId,
    pub(super) authority_epoch: u64,
    pub(super) created_at: u64,
    pub(super) protocol_id: [u8; 16],
    pub(super) version_major: u16,
    pub(super) version_minor: u16,
}

impl NewApplicationSession {
    /// Creates checked session facts.
    ///
    /// # Errors
    ///
    /// Returns invalid input for zero epoch, timestamp, protocol, or major version.
    pub fn new(
        session_id: SessionId,
        actor_id: ActorId,
        authority_epoch: u64,
        created_at: u64,
        protocol_id: [u8; 16],
        version_major: u16,
        version_minor: u16,
    ) -> Result<Self, JournalError> {
        if authority_epoch == 0 || created_at == 0 || protocol_id == [0; 16] || version_major == 0 {
            return Err(invalid("application session contains a zero required value"));
        }
        Ok(Self {
            session_id,
            actor_id,
            authority_epoch,
            created_at,
            protocol_id,
            version_major,
            version_minor,
        })
    }
}

/// One durable negotiated session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationSession {
    pub(super) session_id: SessionId,
    pub(super) actor_id: ActorId,
    pub(super) authority_epoch: u64,
    pub(super) state: ApplicationSessionState,
    pub(super) created_at: u64,
    pub(super) protocol_id: [u8; 16],
    pub(super) version_major: u16,
    pub(super) version_minor: u16,
}

impl ApplicationSession {
    /// Returns the durable session identity.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Returns the authenticated actor.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }

    /// Returns the authority epoch captured at admission.
    #[must_use]
    pub const fn authority_epoch(&self) -> u64 {
        self.authority_epoch
    }

    /// Returns session lifecycle state.
    #[must_use]
    pub const fn state(&self) -> ApplicationSessionState {
        self.state
    }

    /// Returns the monotonic creation tick.
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    /// Returns the negotiated protocol identifier.
    #[must_use]
    pub const fn protocol_id(&self) -> &[u8; 16] {
        &self.protocol_id
    }

    /// Returns the negotiated major version.
    #[must_use]
    pub const fn version_major(&self) -> u16 {
        self.version_major
    }

    /// Returns the negotiated minor version.
    #[must_use]
    pub const fn version_minor(&self) -> u16 {
        self.version_minor
    }
}

const fn invalid(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "validate application ledger value", detail)
}
