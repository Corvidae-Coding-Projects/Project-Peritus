//! Move-only logical capability issuance transitions.

use crate::{Capability, CapabilityIssuancePlan};

use peritus_types::{CommandId, Sha256Digest};

#[doc = " Logical issuance transition to be independently validated and durably committed by later layers."]
#[doc = ""]
#[doc = " Its fields are intentionally private outside this crate:"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " let _forged = peritus_policy::CapabilityIssuanceTransition {"]
#[doc = "     command_id: loop {},"]
#[doc = "     transition_digest: loop {},"]
#[doc = "     capability: loop {},"]
#[doc = " };"]
#[doc = " ```"]
#[doc = ""]
#[doc = " A successful issuance transition cannot be cloned or implicitly copied:"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " fn requires_clone<T: Clone>() {}"]
#[doc = " requires_clone::<peritus_policy::CapabilityIssuanceTransition>();"]
#[doc = " ```"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " fn requires_copy<T: Copy>() {}"]
#[doc = " requires_copy::<peritus_policy::CapabilityIssuanceTransition>();"]
#[doc = " ```"]
#[derive(Debug, Eq, PartialEq)]
pub struct CapabilityIssuanceTransition {
    command_id: CommandId,
    transition_digest: Sha256Digest,
    capability: Capability,
}
impl CapabilityIssuanceTransition {
    #[doc = " Returns the exact unique command identity bound to this issuance attempt."]
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    #[doc = " Returns the exact transition digest supplied by the transition planner."]
    #[must_use]
    pub const fn transition_digest(&self) -> Sha256Digest {
        self.transition_digest
    }
    #[doc = " Borrows the logically issued capability without making it clonable."]
    #[must_use]
    pub const fn capability(&self) -> &Capability {
        &self.capability
    }
    #[doc = " Consumes the transition and returns its move-only logical capability."]
    #[must_use]
    pub fn into_capability(self) -> Capability {
        self.capability
    }
}
impl CapabilityIssuancePlan {
    #[doc = " Constructs a move-only logical issuance transition bound to an exact transition digest."]
    #[doc = ""]
    #[doc = " The returned value is not proof of persistence and is not an effect permit."]
    #[must_use]
    pub fn issue(
        self,
        command_id: CommandId,
        transition_digest: Sha256Digest,
    ) -> CapabilityIssuanceTransition {
        let (scope, issued_at, time_state) = self.into_parts();
        CapabilityIssuanceTransition {
            command_id,
            transition_digest,
            capability: Capability::from_issuance(
                scope,
                issued_at,
                transition_digest,
                command_id,
                time_state,
            ),
        }
    }
}
