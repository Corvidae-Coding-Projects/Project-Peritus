//! Checked application-command ledger values.

use peritus_types::{ActorId, CommandId, SessionId, Sha256Digest};

use crate::{CommittedBatch, JournalError, JournalErrorKind};

/// Opaque nonzero application request identity retained in the command ledger.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ApplicationRequestId([u8; 16]);

impl ApplicationRequestId {
    /// Creates an application request identity.
    ///
    /// # Errors
    ///
    /// Returns invalid input when all bytes are zero.
    pub fn new(bytes: [u8; 16]) -> Result<Self, JournalError> {
        if bytes == [0; 16] {
            Err(invalid("application request identity must be nonzero"))
        } else {
            Ok(Self(bytes))
        }
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Durable command lifecycle state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationCommandState {
    /// Admitted before domain dispatch.
    Pending,
    /// The append acknowledgement was ambiguous and must be reconciled.
    Indeterminate,
    /// The exact committed range and response are retained.
    Committed,
    /// A stable pre-append rejection and response are retained.
    Rejected,
}

impl ApplicationCommandState {
    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::Pending),
            2 => Some(Self::Indeterminate),
            3 => Some(Self::Committed),
            4 => Some(Self::Rejected),
            _ => None,
        }
    }
}

/// New actor/session/key command admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewApplicationCommand {
    pub(super) actor_id: ActorId,
    pub(super) session_id: SessionId,
    pub(super) idempotency_key: Vec<u8>,
    pub(super) request_digest: Sha256Digest,
    pub(super) domain_command_digest: Sha256Digest,
    pub(super) request_id: ApplicationRequestId,
    pub(super) command_id: CommandId,
}

impl NewApplicationCommand {
    /// Creates a bounded ledger admission.
    ///
    /// # Errors
    ///
    /// Returns invalid input unless the key contains 1 through 256 bytes.
    pub fn new(
        actor_id: ActorId,
        session_id: SessionId,
        idempotency_key: Vec<u8>,
        request_digest: Sha256Digest,
        domain_command_digest: Sha256Digest,
        request_id: ApplicationRequestId,
        command_id: CommandId,
    ) -> Result<Self, JournalError> {
        if idempotency_key.is_empty() || idempotency_key.len() > 256 {
            return Err(invalid("application idempotency key is outside the production bound"));
        }
        Ok(Self {
            actor_id,
            session_id,
            idempotency_key,
            request_digest,
            domain_command_digest,
            request_id,
            command_id,
        })
    }
}

/// One durable application command ledger row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationCommandRecord {
    pub(super) actor_id: ActorId,
    pub(super) session_id: SessionId,
    pub(super) idempotency_key: Vec<u8>,
    pub(super) request_digest: Sha256Digest,
    pub(super) domain_command_digest: Sha256Digest,
    pub(super) request_id: ApplicationRequestId,
    pub(super) command_id: CommandId,
    pub(super) state: ApplicationCommandState,
    pub(super) first_position: Option<u64>,
    pub(super) last_position: Option<u64>,
    pub(super) error_code: Option<String>,
    pub(super) result_digest: Option<Sha256Digest>,
}

impl ApplicationCommandRecord {
    /// Returns the actor identity.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }

    /// Returns the session identity.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Borrows the exact idempotency key.
    #[must_use]
    pub fn idempotency_key(&self) -> &[u8] {
        &self.idempotency_key
    }

    /// Returns the complete application request digest.
    #[must_use]
    pub const fn request_digest(&self) -> Sha256Digest {
        self.request_digest
    }

    /// Returns SHA-256 over the exact canonical B3 domain-command frame.
    #[must_use]
    pub const fn domain_command_digest(&self) -> Sha256Digest {
        self.domain_command_digest
    }

    /// Returns the original application request identity.
    #[must_use]
    pub const fn request_id(&self) -> ApplicationRequestId {
        self.request_id
    }

    /// Returns the reserved B3 command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }

    /// Returns command lifecycle state.
    #[must_use]
    pub const fn state(&self) -> ApplicationCommandState {
        self.state
    }

    /// Returns the first exact committed position when committed.
    #[must_use]
    pub const fn first_position(&self) -> Option<u64> {
        self.first_position
    }

    /// Returns the last exact committed position when committed.
    #[must_use]
    pub const fn last_position(&self) -> Option<u64> {
        self.last_position
    }

    /// Borrows the stable rejection code when rejected.
    #[must_use]
    pub fn error_code(&self) -> Option<&str> {
        self.error_code.as_deref()
    }

    /// Returns the exact final response-facts digest when terminal.
    #[must_use]
    pub const fn result_digest(&self) -> Option<Sha256Digest> {
        self.result_digest
    }
}

/// Result of durable idempotency admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationCommandAdmission {
    /// A new pending row was durably inserted.
    Inserted(ApplicationCommandRecord),
    /// The key and exact request digest already exist.
    Existing(ApplicationCommandRecord),
    /// The actor/session/key tuple is bound to a different digest.
    Conflict(ApplicationCommandRecord),
}

/// Terminal or recoverable settlement applied to an admitted command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationCommandSettlement {
    pub(super) kind: SettlementKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SettlementKind {
    Indeterminate,
    Committed { first_position: u64, last_position: u64, result_digest: Sha256Digest },
    Rejected { error_code: String, result_digest: Sha256Digest },
}

impl ApplicationCommandSettlement {
    /// Creates a recovery-required indeterminate settlement.
    #[must_use]
    pub const fn indeterminate() -> Self {
        Self { kind: SettlementKind::Indeterminate }
    }

    /// Creates a committed settlement from a real checked C0 commit receipt.
    #[must_use]
    pub const fn committed(batch: &CommittedBatch, result_digest: Sha256Digest) -> Self {
        Self {
            kind: SettlementKind::Committed {
                first_position: batch.first_position(),
                last_position: batch.last_position(),
                result_digest,
            },
        }
    }

    /// Creates a checked stable rejection settlement.
    ///
    /// # Errors
    ///
    /// Returns invalid input unless the stable code contains 1 through 128 ASCII graphic bytes.
    pub fn rejected(error_code: String, result_digest: Sha256Digest) -> Result<Self, JournalError> {
        if error_code.is_empty()
            || error_code.len() > 128
            || !error_code.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(invalid("application command rejection code is invalid"));
        }
        Ok(Self { kind: SettlementKind::Rejected { error_code, result_digest } })
    }
}

const fn invalid(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "validate application ledger value", detail)
}
