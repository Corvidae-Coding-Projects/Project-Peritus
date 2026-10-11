//! Exact admission and lookup for node role visibility.

use crate::{ContextError, ContextErrorKind, ContextLimits};
use peritus_policy::ActorRole;

#[doc = " Nonempty, canonically ordered set of B1 roles allowed to see a node."]
#[derive(Debug, Eq, PartialEq)]
pub struct RoleVisibility {
    roles: Vec<ActorRole>,
}
impl RoleVisibility {
    #[doc = " Checks nonemptiness, the configured bound, uniqueness, and canonical ordering."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the exact first typed collection or bound error."]
    pub fn new(roles: Vec<ActorRole>, limits: ContextLimits) -> Result<Self, ContextError> {
        if roles.is_empty() {
            return Err(ContextError::plain(ContextErrorKind::EmptyCollection));
        }
        if roles.len() > limits.max_visibility_roles() {
            return Err(ContextError::with_numbers(
                ContextErrorKind::TooManyVisibilityRoles,
                limits.max_visibility_roles() as u64,
                roles.len() as u64,
            ));
        }
        validate_order(&roles)?;
        Ok(Self { roles })
    }
    #[doc = " Borrows the canonical roles."]
    #[must_use]
    pub const fn roles(&self) -> &[ActorRole] {
        self.roles.as_slice()
    }
    #[doc = " Returns whether the exact B1 role is present."]
    #[must_use]
    pub fn contains(&self, role: ActorRole) -> bool {
        let target = role_rank(role);
        let mut index = 0;
        while index < self.roles.len() {
            if role_rank(self.roles[index]) == target {
                return true;
            }
            index += 1;
        }
        false
    }
}
fn validate_order(roles: &[ActorRole]) -> Result<(), ContextError> {
    let mut index = 1;
    while index < roles.len() {
        let previous = role_rank(roles[index - 1]);
        let current = role_rank(roles[index]);
        if previous == current {
            return Err(ContextError::plain(ContextErrorKind::DuplicateValue));
        }
        if previous > current {
            return Err(ContextError::plain(ContextErrorKind::NonCanonicalOrder));
        }
        index += 1;
    }
    Ok(())
}
impl Clone for RoleVisibility {
    fn clone(&self) -> Self {
        Self { roles: self.roles.clone() }
    }
}
const fn role_rank(role: ActorRole) -> u8 {
    match role {
        ActorRole::Writer => 0,
        ActorRole::Fixer => 1,
        ActorRole::Reviewer => 2,
        ActorRole::Evaluator => 3,
        ActorRole::GateRunner => 4,
        ActorRole::Orchestrator => 5,
        ActorRole::EvolutionAgent => 6,
        ActorRole::HumanAuthority => 7,
        ActorRole::DaemonService => 8,
        ActorRole::ProviderToolWorker => 9,
        ActorRole::Plugin => 10,
    }
}
