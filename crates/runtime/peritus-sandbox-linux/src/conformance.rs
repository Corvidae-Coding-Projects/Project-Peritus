//! Platform-neutral facts consumed by fresh-subject conformance adapters.

use crate::{LinuxBackendDescriptor, LinuxProbe};
use peritus_sandbox::{BackendAdmission, CheckedSandboxPlan};
use peritus_types::Sha256Digest;

#[cfg(all(test, target_os = "linux"))]
mod tests;

/// Exact conformance binding without exposing platform effects or authority constructors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConformanceFacts {
    plan_digest: Sha256Digest,
    descriptor_digest: Sha256Digest,
    support_digest: Sha256Digest,
    probe_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
    support_complete: bool,
}

impl ConformanceFacts {
    /// Projects exact independent preparation facts.
    #[must_use]
    pub const fn new(
        plan: &CheckedSandboxPlan,
        admission: &BackendAdmission,
        descriptor: &LinuxBackendDescriptor,
        probe: &LinuxProbe,
    ) -> Self {
        Self {
            plan_digest: plan.digest(),
            descriptor_digest: descriptor.common().digest(),
            support_digest: descriptor.common().support_digest(),
            probe_digest: probe.digest(),
            preparation_digest: admission.preparation_digest(),
            support_complete: crate::verified::support_covers(
                plan.required_features().bits(),
                descriptor.common().supported_features().bits(),
            ),
        }
    }
    /// Reports exact plan/admission/descriptor and complete support agreement.
    #[must_use]
    pub fn exact(&self, plan: &CheckedSandboxPlan, admission: &BackendAdmission) -> bool {
        self.support_complete
            && self.plan_digest == plan.digest()
            && self.descriptor_digest == admission.descriptor_digest()
            && self.support_digest == admission.support_digest()
            && self.preparation_digest == admission.preparation_digest()
    }
    /// Returns runtime probe identity.
    #[must_use]
    pub const fn probe_digest(self) -> Sha256Digest {
        self.probe_digest
    }
}
