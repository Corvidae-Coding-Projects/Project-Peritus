//! Checked identities and immutable turn binding.

use crate::{AgentErrorCode, AgentOperation, AgentRecovery, AgentRejection};
use peritus_policy::ActorRole;
use peritus_role::RoleProfile;
use peritus_types::{
    ActorId, AttemptId, EnvironmentId, ProviderProfileId, RevisionNumber, RevisionTuple, SessionId,
    Sha256Digest, TurnId,
};

/// Bounded secret-safe text retained in state and errors.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SafeText(String);

impl SafeText {
    pub const MAX_BYTES: usize = 16_384;

    /// Validates a nonempty UTF-8 string with no NUL or control characters other than whitespace.
    ///
    /// # Errors
    ///
    /// Returns `InvalidText` when the content is empty, excessive, or contains forbidden controls.
    pub fn new(value: String) -> Result<Self, AgentRejection> {
        if value.is_empty() || value.len() > Self::MAX_BYTES {
            return Err(AgentRejection::new(
                AgentErrorCode::InvalidText,
                AgentOperation::ValidateCompletion,
                AgentRecovery::CorrectRequest,
                "text is empty or exceeds the retained-state bound",
            ));
        }
        if value.chars().any(|character| {
            character == '\0' || (character.is_control() && !character.is_whitespace())
        }) {
            return Err(AgentRejection::new(
                AgentErrorCode::InvalidText,
                AgentOperation::ValidateCompletion,
                AgentRecovery::CorrectRequest,
                "text contains a forbidden control character",
            ));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Positive immutable provider-profile revision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProfileRevision(u64);

impl ProfileRevision {
    /// Creates a positive profile revision.
    ///
    /// # Errors
    ///
    /// Returns `InvalidBinding` when `value` is zero.
    pub const fn new(value: u64) -> Result<Self, AgentRejection> {
        if value == 0 {
            Err(AgentRejection::new(
                AgentErrorCode::InvalidBinding,
                AgentOperation::ValidateBinding,
                AgentRecovery::CorrectRequest,
                "provider profile revision must be positive",
            ))
        } else {
            Ok(Self(value))
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Nonzero digest projection of a provider call identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModelCallId(Sha256Digest);

impl ModelCallId {
    /// Creates an opaque nonzero model-call digest.
    ///
    /// # Errors
    ///
    /// Returns `InvalidBinding` when the supplied digest is all zero.
    pub fn new(value: Sha256Digest) -> Result<Self, AgentRejection> {
        if value.as_bytes().iter().all(|byte| *byte == 0) {
            Err(AgentRejection::new(
                AgentErrorCode::InvalidBinding,
                AgentOperation::Reduce,
                AgentRecovery::CorrectRequest,
                "model call identity digest must be nonzero",
            ))
        } else {
            Ok(Self(value))
        }
    }

    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }
}

/// Zero-based canonical tool-call ordinal.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ToolOrdinal(u16);

impl ToolOrdinal {
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Immutable authority and revision binding for one writer or fixer turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentBinding {
    turn_id: TurnId,
    attempt_id: AttemptId,
    actor_id: ActorId,
    role: ActorRole,
    role_profile: RoleProfile,
    session_id: SessionId,
    environment_id: EnvironmentId,
    revision: RevisionTuple,
    provider_profile_id: ProviderProfileId,
    provider_profile_revision: ProfileRevision,
    limits_revision: RevisionNumber,
}

impl AgentBinding {
    /// Creates an exact turn binding and rejects role/profile/revision drift.
    ///
    /// # Errors
    ///
    /// Returns `InvalidBinding` when role, profile, or provider revision facts disagree.
    #[allow(
        clippy::too_many_arguments,
        reason = "the authority binding must remain explicit and auditable"
    )]
    pub fn new(
        turn_id: TurnId,
        attempt_id: AttemptId,
        actor_id: ActorId,
        role: ActorRole,
        role_profile: RoleProfile,
        session_id: SessionId,
        environment_id: EnvironmentId,
        revision: RevisionTuple,
        provider_profile_id: ProviderProfileId,
        provider_profile_revision: ProfileRevision,
        limits_revision: RevisionNumber,
    ) -> Result<Self, AgentRejection> {
        if !matches!(role, ActorRole::Writer | ActorRole::Fixer) {
            return Err(binding_error("agent loop role must be writer or fixer"));
        }
        if role_profile.actor_role() != role {
            return Err(binding_error("role profile does not match actor role"));
        }
        if revision.provider_profile_id() != provider_profile_id {
            return Err(binding_error("revision tuple does not match provider profile"));
        }
        Ok(Self {
            turn_id,
            attempt_id,
            actor_id,
            role,
            role_profile,
            session_id,
            environment_id,
            revision,
            provider_profile_id,
            provider_profile_revision,
            limits_revision,
        })
    }

    #[must_use]
    pub const fn turn_id(&self) -> TurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn attempt_id(&self) -> AttemptId {
        self.attempt_id
    }
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    #[must_use]
    pub const fn role(&self) -> ActorRole {
        self.role
    }
    #[must_use]
    pub const fn role_profile(&self) -> &RoleProfile {
        &self.role_profile
    }
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    pub const fn provider_profile_id(&self) -> ProviderProfileId {
        self.provider_profile_id
    }
    #[must_use]
    pub const fn provider_profile_revision(&self) -> ProfileRevision {
        self.provider_profile_revision
    }
    #[must_use]
    pub const fn limits_revision(&self) -> RevisionNumber {
        self.limits_revision
    }
}

const fn binding_error(detail: &'static str) -> AgentRejection {
    AgentRejection::new(
        AgentErrorCode::InvalidBinding,
        AgentOperation::ValidateBinding,
        AgentRecovery::CorrectRequest,
        detail,
    )
}
