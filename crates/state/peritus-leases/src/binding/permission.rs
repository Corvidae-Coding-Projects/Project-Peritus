//! Owned exact unprivileged permission evidence.

use peritus_types::{CapabilityName, ResourceId};

#[doc = " Owned exact permission projection used by durable lease-use command bindings."]
#[doc = ""]
#[doc = " It is intentionally unprivileged: construction of this value never creates a policy"]
#[doc = " capability, lease transition, durable receipt, or effect permit."]
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct LeasePermissionBinding {
    pub(crate) resource_id: ResourceId,
    pub(crate) capability_name: CapabilityName,
}
impl LeasePermissionBinding {
    pub(crate) fn from_permission(permission: &peritus_policy::Permission) -> Self {
        Self {
            resource_id: permission.resource_id(),
            capability_name: permission.capability_name().clone(),
        }
    }
    #[doc = " Returns the exact resolved resource identity."]
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.resource_id
    }
    #[doc = " Borrows the exact canonical capability name."]
    #[must_use]
    pub const fn capability_name(&self) -> &CapabilityName {
        &self.capability_name
    }
}
