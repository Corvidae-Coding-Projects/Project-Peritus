//! Immutable exact B1 and C6 role ownership agreement.

use peritus_policy::ActorRole;
use peritus_role::HarnessRole;
use peritus_types::ActorId;

use crate::{
    OrchestratorError, OrchestratorErrorKind, OrchestratorLimits, OrchestratorRecoveryAction,
};

/// One actor whose B1 and C6 identities agree exactly.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RoleAssignment {
    actor: ActorId,
    actor_role: ActorRole,
    harness_role: HarnessRole,
}

impl RoleAssignment {
    /// Creates one exact B1/C6 role assignment.
    ///
    /// # Errors
    /// Rejects a harness role whose canonical B1 mapping differs.
    pub fn new(
        actor: ActorId,
        actor_role: ActorRole,
        harness_role: HarnessRole,
    ) -> Result<Self, OrchestratorError> {
        let value = Self::from_wire(actor, actor_role, harness_role);
        value.validate()?;
        Ok(value)
    }

    pub(crate) const fn from_wire(
        actor: ActorId,
        actor_role: ActorRole,
        harness_role: HarnessRole,
    ) -> Self {
        Self { actor, actor_role, harness_role }
    }

    pub(crate) fn validate(self) -> Result<(), OrchestratorError> {
        if self.actor_role == self.harness_role.actor_role() {
            Ok(())
        } else {
            Err(binding_error("B1 actor role differs from its canonical C6 harness role"))
        }
    }

    /// Returns assigned actor.
    #[must_use]
    pub const fn actor(self) -> ActorId {
        self.actor
    }
    /// Returns exact B1 security role.
    #[must_use]
    pub const fn actor_role(self) -> ActorRole {
        self.actor_role
    }
    /// Returns exact C6 harness role.
    #[must_use]
    pub const fn harness_role(self) -> HarnessRole {
        self.harness_role
    }
}

/// Exact actor ownership for the service, mutators, and independent reviewer pool.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleOwnership {
    service_actor: ActorId,
    service_role: ActorRole,
    writer: RoleAssignment,
    fixer: RoleAssignment,
    reviewers: Vec<RoleAssignment>,
}

impl RoleOwnership {
    /// Creates a canonical role-separated ownership binding.
    ///
    /// # Errors
    /// Rejects role mismatch, duplicated principals, or empty/noncanonical reviewer pools.
    pub fn new(
        service_actor: ActorId,
        service_role: ActorRole,
        writer: RoleAssignment,
        fixer: RoleAssignment,
        reviewers: Vec<RoleAssignment>,
        limits: OrchestratorLimits,
    ) -> Result<Self, OrchestratorError> {
        let value = Self::from_wire(service_actor, service_role, writer, fixer, reviewers);
        value.validate(limits)?;
        Ok(value)
    }

    pub(crate) const fn from_wire(
        service_actor: ActorId,
        service_role: ActorRole,
        writer: RoleAssignment,
        fixer: RoleAssignment,
        reviewers: Vec<RoleAssignment>,
    ) -> Self {
        Self { service_actor, service_role, writer, fixer, reviewers }
    }

    pub(crate) fn validate(&self, limits: OrchestratorLimits) -> Result<(), OrchestratorError> {
        self.writer.validate()?;
        self.fixer.validate()?;
        for reviewer in &self.reviewers {
            reviewer.validate()?;
        }
        let fixed_roles = self.service_role == ActorRole::Orchestrator
            && self.writer.harness_role == HarnessRole::Writer
            && self.fixer.harness_role == HarnessRole::Fixer
            && self.reviewers.iter().all(|reviewer| {
                reviewer.harness_role == HarnessRole::Reviewer
                    && reviewer.actor_role == ActorRole::Reviewer
            });
        let principals_separate = self.service_actor != self.writer.actor
            && self.service_actor != self.fixer.actor
            && self.writer.actor != self.fixer.actor;
        let reviewers_valid = !self.reviewers.is_empty()
            && self.reviewers.len() <= usize::from(limits.child_directives())
            && self.reviewers.windows(2).all(|pair| pair[0].actor < pair[1].actor)
            && self.reviewers.iter().all(|reviewer| {
                reviewer.actor != self.service_actor
                    && reviewer.actor != self.writer.actor
                    && reviewer.actor != self.fixer.actor
            });
        if fixed_roles && principals_separate && reviewers_valid {
            Ok(())
        } else {
            Err(binding_error(
                "orchestrator role mappings or principals are mismatched, duplicated, or oversized",
            ))
        }
    }

    /// Returns E0 service actor.
    #[must_use]
    pub const fn service_actor(&self) -> ActorId {
        self.service_actor
    }
    /// Returns exact E0 B1 service role.
    #[must_use]
    pub const fn service_role(&self) -> ActorRole {
        self.service_role
    }
    /// Returns writer assignment.
    #[must_use]
    pub const fn writer(&self) -> RoleAssignment {
        self.writer
    }
    /// Returns fixer assignment.
    #[must_use]
    pub const fn fixer(&self) -> RoleAssignment {
        self.fixer
    }
    /// Borrows canonical independent reviewer assignments.
    #[must_use]
    pub fn reviewers(&self) -> &[RoleAssignment] {
        &self.reviewers
    }

    /// Finds a reviewer assignment by actor identity.
    #[must_use]
    pub fn reviewer(&self, actor: ActorId) -> Option<RoleAssignment> {
        self.reviewers
            .binary_search_by_key(&actor, |assignment| assignment.actor)
            .ok()
            .map(|index| self.reviewers[index])
    }
}

const fn binding_error(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::BindingMismatch,
        OrchestratorRecoveryAction::CorrectInput,
        detail,
    )
}
