//! Successful exact action-bound capability-use transitions.

use crate::{
    AuthorityInstant, Capability, CapabilityScope, CapabilityUseRequest, Permission, UseLimit,
};

use peritus_types::{ActionId, Sha256Digest};

#[doc = " Successful move-only logical capability-use transition for one exact action and permission."]
#[doc = ""]
#[doc = " Its fields are intentionally private outside this crate:"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " let _forged = peritus_policy::CapabilityUseTransition {"]
#[doc = "     action_id: loop {},"]
#[doc = "     action_digest: loop {},"]
#[doc = "     permission: loop {},"]
#[doc = "     used_at: loop {},"]
#[doc = "     transition_digest: loop {},"]
#[doc = "     previous_remaining: loop {},"]
#[doc = "     successor: loop {},"]
#[doc = " };"]
#[doc = " ```"]
#[doc = ""]
#[doc = " A successful transition cannot be cloned or implicitly copied:"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " fn requires_clone<T: Clone>() {}"]
#[doc = " requires_clone::<peritus_policy::CapabilityUseTransition>();"]
#[doc = " ```"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " fn requires_copy<T: Copy>() {}"]
#[doc = " requires_copy::<peritus_policy::CapabilityUseTransition>();"]
#[doc = " ```"]
#[derive(Debug, Eq, PartialEq)]
pub struct CapabilityUseTransition {
    pub(crate) action_id: ActionId,
    pub(crate) action_digest: Sha256Digest,
    pub(crate) permission: Permission,
    pub(crate) used_at: AuthorityInstant,
    pub(crate) transition_digest: Sha256Digest,
    pub(crate) previous_remaining: UseLimit,
    pub(crate) successor: Capability,
}
impl CapabilityUseTransition {
    pub(crate) fn new(
        request: CapabilityUseRequest,
        transition_digest: Sha256Digest,
        previous_remaining: UseLimit,
        successor: Capability,
    ) -> Self {
        let (action_id, action_digest, permission, _, _, _, _, used_at) = request.into_parts();
        Self {
            action_id,
            action_digest,
            permission,
            used_at,
            transition_digest,
            previous_remaining,
            successor,
        }
    }
    #[doc = " Returns the exact action identity."]
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    #[doc = " Returns the exact action digest."]
    #[must_use]
    pub const fn action_digest(&self) -> Sha256Digest {
        self.action_digest
    }
    #[doc = " Returns the exact capability permission consumed for the action."]
    #[must_use]
    pub const fn permission(&self) -> &Permission {
        &self.permission
    }
    #[doc = " Returns the complete exact capability scope preserved by the transition."]
    #[must_use]
    pub const fn scope(&self) -> &CapabilityScope {
        self.successor.scope()
    }
    #[doc = " Returns the accepted authority-time observation for this use."]
    #[must_use]
    pub const fn used_at(&self) -> AuthorityInstant {
        self.used_at
    }
    #[doc = " Returns the exact transition digest supplied by the transition planner."]
    #[must_use]
    pub const fn transition_digest(&self) -> Sha256Digest {
        self.transition_digest
    }
    #[doc = " Returns the use bound before this one successful use."]
    #[must_use]
    pub const fn previous_remaining(&self) -> UseLimit {
        self.previous_remaining
    }
    #[doc = " Borrows the move-only successor capability."]
    #[must_use]
    pub const fn successor(&self) -> &Capability {
        &self.successor
    }
    #[doc = " Consumes the transition and returns the move-only successor capability."]
    #[must_use]
    pub fn into_successor(self) -> Capability {
        self.successor
    }
}
