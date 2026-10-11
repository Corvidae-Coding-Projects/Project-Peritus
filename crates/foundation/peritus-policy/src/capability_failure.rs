//! Rejected capability uses that preserve the move-only authority value.

use crate::{Capability, PolicyError};

#[doc = " Rejected capability-use transition that preserves ownership of the unchanged capability."]
#[derive(Debug, Eq, PartialEq)]
pub struct CapabilityUseFailure {
    error: PolicyError,
    capability: Capability,
}
impl CapabilityUseFailure {
    pub(crate) const fn new(error: PolicyError, capability: Capability) -> Self {
        Self { error, capability }
    }
    #[doc = " Returns the exact typed failure."]
    #[must_use]
    pub const fn error(&self) -> PolicyError {
        self.error
    }
    #[doc = " Borrows the unchanged prior capability."]
    #[must_use]
    pub const fn capability(&self) -> &Capability {
        &self.capability
    }
    #[doc = " Consumes the failure and returns the unchanged prior capability."]
    #[must_use]
    pub fn into_capability(self) -> Capability {
        self.capability
    }
}
