//! Fail-closed backend admission.

use crate::{
    BackendDescriptor, BackendKind, CheckedSandboxPlan, RecoveryClass, SandboxError,
    SandboxErrorKind, SandboxOperation,
};
use peritus_types::Sha256Digest;

/// Admission context controls whether a reference-only backend is acceptable.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AdmissionProfile {
    /// Requires a native backend suitable for real process execution.
    Production,
    /// Allows the deterministic reference backend for conformance tests.
    Conformance,
}

impl AdmissionProfile {
    const fn ordinal(self) -> u8 {
        match self {
            Self::Production => 0,
            Self::Conformance => 1,
        }
    }
}

/// Immutable evidence that one descriptor covers one checked plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendAdmission {
    descriptor: BackendDescriptor,
    plan_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
}

impl BackendAdmission {
    /// Returns the admitted descriptor.
    #[must_use]
    pub const fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }
    /// Returns the admitted descriptor digest.
    #[must_use]
    pub const fn descriptor_digest(&self) -> Sha256Digest {
        self.descriptor.digest()
    }
    /// Returns the admitted support digest.
    #[must_use]
    pub const fn support_digest(&self) -> Sha256Digest {
        self.descriptor.support_digest()
    }
    /// Returns the checked plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }
    /// Returns the deterministic preparation binding digest.
    #[must_use]
    pub const fn preparation_digest(&self) -> Sha256Digest {
        self.preparation_digest
    }
}

/// Admits a backend only when it can enforce every required feature.
///
/// # Errors
/// Returns `UnsupportedBackend` with exact missing features or when a reference backend is used in
/// production.
pub fn admit_backend(
    plan: &CheckedSandboxPlan,
    descriptor: &BackendDescriptor,
    profile: AdmissionProfile,
) -> Result<BackendAdmission, SandboxError> {
    let missing = plan.required_features().missing_from(descriptor.supported_features());
    let facts = crate::verified::BackendFacts {
        required_feature_bits: plan.required_features().bits(),
        supported_feature_bits: descriptor.supported_features().bits(),
        profile_ordinal: profile.ordinal(),
        backend_kind_ordinal: descriptor.kind().ordinal(),
    };
    if missing.bits() != 0 {
        return Err(SandboxError::unsupported(missing, "backend lacks required features"));
    }
    if profile == AdmissionProfile::Production && descriptor.kind() == BackendKind::ReferenceOnly {
        return Err(SandboxError::unsupported(
            plan.required_features(),
            "reference backend is not a production enforcer",
        ));
    }
    if !crate::verified::backend_complete(facts) {
        return Err(SandboxError::new(
            SandboxErrorKind::BackendMismatch,
            SandboxOperation::AdmitBackend,
            RecoveryClass::Replan,
            "backend refinement projection is incomplete",
        ));
    }
    let preparation_digest = crate::canonical::preparation_digest(
        plan.digest(),
        descriptor.digest(),
        descriptor.support_digest(),
    );
    Ok(BackendAdmission {
        descriptor: descriptor.clone(),
        plan_digest: plan.digest(),
        preparation_digest,
    })
}
