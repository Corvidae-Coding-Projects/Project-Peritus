//! Owned exact unprivileged permission evidence.

use peritus_types::{CapabilityName, ResourceId};
use vstd::prelude::*;

verus! {

/// Owned exact permission projection used by durable lease-use command bindings.
///
/// It is intentionally unprivileged: construction of this value never creates a policy
/// capability, lease transition, durable receipt, or effect permit.
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct LeasePermissionBinding {
    pub(crate) resource_id: ResourceId,
    pub(crate) capability_name: CapabilityName,
}

impl LeasePermissionBinding {
    pub(crate) fn from_permission(
        permission: &peritus_policy::Permission,
    ) -> (binding: Self)
        ensures binding.matches_permission(permission),
    {
        Self {
            resource_id: permission.resource_id(),
            capability_name: permission.capability_name().clone(),
        }
    }

    pub(crate) open spec fn matches_permission(
        &self,
        permission: &peritus_policy::Permission,
    ) -> bool {
        self.resource_id.spec_bytes() == permission.spec_resource_id()
            && self.capability_name.spec_bytes() == permission.spec_capability_name()
    }

    /// Returns the exact resolved resource identity.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId { self.resource_id }

    /// Borrows the exact canonical capability name.
    #[must_use]
    pub const fn capability_name(&self) -> &CapabilityName { &self.capability_name }
}

} // verus!
