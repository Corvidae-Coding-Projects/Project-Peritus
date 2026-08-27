//! Checked durable application-prompt target and settlement values.

use peritus_types::{ActorId, Generation, RevisionTuple, SessionId, Sha256Digest};

use super::ApplicationRequestId;
use crate::{JournalError, JournalErrorKind};

/// Maximum exact canonical A3 prompt-settlement frame retained by the application ledger.
pub const MAX_APPLICATION_PROMPT_SETTLEMENT_BYTES: usize = 16 * 1024 * 1024;
/// Maximum exact canonical envelope-free prompt binding retained for restart reconstruction.
pub const MAX_APPLICATION_PROMPT_BINDING_BYTES: usize = 16 * 1024 * 1024;

/// Opaque nonzero application prompt identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ApplicationPromptId([u8; 16]);

impl ApplicationPromptId {
    /// Creates an application prompt identity.
    ///
    /// # Errors
    ///
    /// Returns invalid input when all bytes are zero.
    pub fn new(bytes: [u8; 16]) -> Result<Self, JournalError> {
        if bytes == [0; 16] {
            Err(invalid("application prompt identity must be nonzero"))
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

/// Closed kind of producer target awaiting a prompt response.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationPromptTargetKind {
    /// A producer awaits one bounded user-input value.
    UserInput,
    /// A producer awaits one externally signed approval decision or cancellation.
    Approval,
}

impl ApplicationPromptTargetKind {
    pub(super) const fn tag(self) -> i64 {
        match self {
            Self::UserInput => 1,
            Self::Approval => 2,
        }
    }

    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::UserInput),
            2 => Some(Self::Approval),
            _ => None,
        }
    }
}

/// Closed durable prompt lifecycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationPromptState {
    /// The target is durable and awaits one response.
    Awaiting,
    /// An answer was durably admitted for the target.
    Answered,
    /// A cancellation was durably admitted for the target.
    Cancelled,
}

impl ApplicationPromptState {
    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::Awaiting),
            2 => Some(Self::Answered),
            3 => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// Closed kind of exact response durably delivered to a prompt target.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationPromptSettlementKind {
    /// Bounded user input.
    UserInput,
    /// Strictly authenticated signed approval input.
    Approval,
    /// Unprivileged exact cancellation.
    Cancellation,
}

impl ApplicationPromptSettlementKind {
    pub(super) const fn tag(self) -> i64 {
        match self {
            Self::UserInput => 1,
            Self::Approval => 2,
            Self::Cancellation => 3,
        }
    }

    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::UserInput),
            2 => Some(Self::Approval),
            3 => Some(Self::Cancellation),
            _ => None,
        }
    }
}

/// New immutable durable prompt target registered before publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewApplicationPromptTarget {
    pub(super) prompt_id: ApplicationPromptId,
    pub(super) actor_id: ActorId,
    pub(super) session_id: SessionId,
    pub(super) originating_request_id: ApplicationRequestId,
    pub(super) target_kind: ApplicationPromptTargetKind,
    pub(super) revision: RevisionTuple,
    pub(super) freshness_digest: Sha256Digest,
    pub(super) cancellation_generation: Generation,
    pub(super) binding_digest: Sha256Digest,
    pub(super) binding_bytes: Vec<u8>,
    pub(super) maximum_answer_bytes: usize,
}

impl NewApplicationPromptTarget {
    /// Creates one complete target binding.
    ///
    /// # Errors
    ///
    /// Returns invalid input for a zero or unrepresentable answer bound.
    #[allow(
        clippy::too_many_arguments,
        reason = "the complete prompt authority binding is explicit"
    )]
    pub fn new(
        prompt_id: ApplicationPromptId,
        actor_id: ActorId,
        session_id: SessionId,
        originating_request_id: ApplicationRequestId,
        target_kind: ApplicationPromptTargetKind,
        revision: RevisionTuple,
        freshness_digest: Sha256Digest,
        cancellation_generation: Generation,
        binding_digest: Sha256Digest,
        binding_bytes: Vec<u8>,
        maximum_answer_bytes: usize,
    ) -> Result<Self, JournalError> {
        if maximum_answer_bytes == 0 || maximum_answer_bytes > 1024 * 1024 {
            return Err(invalid("application prompt answer bound is outside production limits"));
        }
        if binding_bytes.is_empty()
            || binding_bytes.len() > MAX_APPLICATION_PROMPT_BINDING_BYTES
            || peritus_codec::sha256(&binding_bytes) != binding_digest
        {
            return Err(invalid(
                "application prompt binding bytes are empty, oversized, or digest-mismatched",
            ));
        }
        Ok(Self {
            prompt_id,
            actor_id,
            session_id,
            originating_request_id,
            target_kind,
            revision,
            freshness_digest,
            cancellation_generation,
            binding_digest,
            binding_bytes,
            maximum_answer_bytes,
        })
    }
}

/// Exact canonical A3 response accepted for durable target settlement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationPromptSettlement {
    pub(super) kind: ApplicationPromptSettlementKind,
    pub(super) request_id: ApplicationRequestId,
    pub(super) digest: Sha256Digest,
    pub(super) bytes: Vec<u8>,
}

impl ApplicationPromptSettlement {
    /// Creates a bounded settlement and verifies its declared digest.
    ///
    /// # Errors
    ///
    /// Returns invalid input for empty, oversized, or digest-mismatched bytes.
    pub fn new(
        kind: ApplicationPromptSettlementKind,
        request_id: ApplicationRequestId,
        digest: Sha256Digest,
        bytes: Vec<u8>,
    ) -> Result<Self, JournalError> {
        if bytes.is_empty() || bytes.len() > MAX_APPLICATION_PROMPT_SETTLEMENT_BYTES {
            return Err(invalid(
                "application prompt settlement bytes are outside production limits",
            ));
        }
        if peritus_codec::sha256(&bytes) != digest {
            return Err(invalid(
                "application prompt settlement digest differs from its exact bytes",
            ));
        }
        Ok(Self { kind, request_id, digest, bytes })
    }

    /// Returns the closed settlement kind.
    #[must_use]
    pub const fn kind(&self) -> ApplicationPromptSettlementKind {
        self.kind
    }

    /// Returns the exact A3 request identity.
    #[must_use]
    pub const fn request_id(&self) -> ApplicationRequestId {
        self.request_id
    }

    /// Returns the exact canonical frame digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Borrows the exact canonical A3 request frame.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// One durable prompt target and optional exact terminal settlement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationPromptRecord {
    pub(super) prompt_id: ApplicationPromptId,
    pub(super) actor_id: ActorId,
    pub(super) session_id: SessionId,
    pub(super) originating_request_id: ApplicationRequestId,
    pub(super) target_kind: ApplicationPromptTargetKind,
    pub(super) revision: RevisionTuple,
    pub(super) freshness_digest: Sha256Digest,
    pub(super) cancellation_generation: Generation,
    pub(super) binding_digest: Sha256Digest,
    pub(super) binding_bytes: Vec<u8>,
    pub(super) maximum_answer_bytes: usize,
    pub(super) state: ApplicationPromptState,
    pub(super) settlement: Option<ApplicationPromptSettlement>,
}

impl ApplicationPromptRecord {
    /// Returns the prompt identity.
    #[must_use]
    pub const fn prompt_id(&self) -> ApplicationPromptId {
        self.prompt_id
    }
    /// Returns the authenticated target actor.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    /// Returns the durable target session.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
    /// Returns the request that originated the prompt.
    #[must_use]
    pub const fn originating_request_id(&self) -> ApplicationRequestId {
        self.originating_request_id
    }
    /// Returns the closed target kind.
    #[must_use]
    pub const fn target_kind(&self) -> ApplicationPromptTargetKind {
        self.target_kind
    }
    /// Returns the current durable target revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the exact target freshness digest.
    #[must_use]
    pub const fn freshness_digest(&self) -> Sha256Digest {
        self.freshness_digest
    }
    /// Returns the current durable cancellation generation.
    #[must_use]
    pub const fn cancellation_generation(&self) -> Generation {
        self.cancellation_generation
    }
    /// Returns the exact canonical prompt-binding digest.
    #[must_use]
    pub const fn binding_digest(&self) -> Sha256Digest {
        self.binding_digest
    }
    /// Borrows the exact canonical envelope-free prompt binding retained for restart.
    #[must_use]
    pub fn binding_bytes(&self) -> &[u8] {
        &self.binding_bytes
    }
    /// Returns the target-specific answer byte ceiling.
    #[must_use]
    pub const fn maximum_answer_bytes(&self) -> usize {
        self.maximum_answer_bytes
    }
    /// Returns the durable prompt lifecycle.
    #[must_use]
    pub const fn state(&self) -> ApplicationPromptState {
        self.state
    }
    /// Borrows the exact terminal settlement when present.
    #[must_use]
    pub const fn settlement(&self) -> Option<&ApplicationPromptSettlement> {
        self.settlement.as_ref()
    }
}

/// Result of durable idempotent prompt-target registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationPromptRegistration {
    /// A new awaiting target was inserted.
    Inserted(ApplicationPromptRecord),
    /// The exact target already exists.
    Existing(ApplicationPromptRecord),
    /// The prompt identity is bound to different immutable target facts.
    Conflict(ApplicationPromptRecord),
}

pub(super) fn target_matches(
    record: &ApplicationPromptRecord,
    target: &NewApplicationPromptTarget,
) -> bool {
    record.prompt_id == target.prompt_id
        && record.actor_id == target.actor_id
        && record.session_id == target.session_id
        && record.originating_request_id == target.originating_request_id
        && record.target_kind == target.target_kind
        && record.revision == target.revision
        && record.freshness_digest == target.freshness_digest
        && record.cancellation_generation == target.cancellation_generation
        && record.binding_digest == target.binding_digest
        && record.binding_bytes == target.binding_bytes
        && record.maximum_answer_bytes == target.maximum_answer_bytes
}

pub(super) fn settlement_matches(
    left: &ApplicationPromptSettlement,
    right: &ApplicationPromptSettlement,
) -> bool {
    left == right
}

const fn invalid(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "validate application prompt", detail)
}
