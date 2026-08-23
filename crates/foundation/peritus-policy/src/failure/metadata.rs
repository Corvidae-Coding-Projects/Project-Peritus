//! Stable diagnostics and recovery metadata for policy failures.

use super::{PolicyError, PolicyErrorKind, RecoveryClass};
use vstd::prelude::*;

verus! {

impl PolicyError {
    /// Returns the stable subsystem diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self.kind {
            PolicyErrorKind::EmptyCanonicalCollection => "PERITUS-POLICY-INPUT-001",
            PolicyErrorKind::NonCanonicalOrder => "PERITUS-POLICY-INPUT-002",
            PolicyErrorKind::DuplicateCanonicalValue => "PERITUS-POLICY-INPUT-003",
            PolicyErrorKind::InvalidValidityWindow => "PERITUS-POLICY-TIME-001",
            PolicyErrorKind::ClockEpochMismatch => "PERITUS-POLICY-TIME-002",
            PolicyErrorKind::ClockRegression => "PERITUS-POLICY-TIME-003",
            PolicyErrorKind::TimeOverflow => "PERITUS-POLICY-TIME-004",
            PolicyErrorKind::ZeroUseLimit => "PERITUS-POLICY-SCOPE-001",
            PolicyErrorKind::SelectorOutsideBoundary => "PERITUS-POLICY-SCOPE-002",
            PolicyErrorKind::InvalidPolicyTier => "PERITUS-POLICY-LAYER-001",
            PolicyErrorKind::InvalidRuleKind => "PERITUS-POLICY-LAYER-002",
            PolicyErrorKind::AmendmentBaseMismatch => "PERITUS-POLICY-AMEND-001",
            PolicyErrorKind::AmendmentPolicyIdReuse => "PERITUS-POLICY-AMEND-002",
            PolicyErrorKind::AmendmentTierMismatch => "PERITUS-POLICY-AMEND-003",
            PolicyErrorKind::CapabilityScopeMismatch => "PERITUS-POLICY-CAPABILITY-001",
            PolicyErrorKind::CapabilityExhausted => "PERITUS-POLICY-CAPABILITY-002",
            PolicyErrorKind::CapabilityNotYetValid => "PERITUS-POLICY-CAPABILITY-003",
            PolicyErrorKind::CapabilityExpired => "PERITUS-POLICY-CAPABILITY-004",
            PolicyErrorKind::PolicyRevisionMismatch => "PERITUS-POLICY-REVISION-001",
            PolicyErrorKind::InvalidOperationRisk => "PERITUS-POLICY-OPERATION-001",
        }
    }

    /// Returns the stable recovery class for this failure.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        match self.kind {
            PolicyErrorKind::ClockEpochMismatch
            | PolicyErrorKind::ClockRegression
            | PolicyErrorKind::TimeOverflow => RecoveryClass::Reobserve,
            PolicyErrorKind::CapabilityScopeMismatch
            | PolicyErrorKind::CapabilityExhausted
            | PolicyErrorKind::CapabilityNotYetValid
            | PolicyErrorKind::CapabilityExpired
            | PolicyErrorKind::AmendmentBaseMismatch
            | PolicyErrorKind::AmendmentPolicyIdReuse => RecoveryClass::Reauthorize,
            PolicyErrorKind::EmptyCanonicalCollection
            | PolicyErrorKind::NonCanonicalOrder
            | PolicyErrorKind::DuplicateCanonicalValue
            | PolicyErrorKind::InvalidValidityWindow
            | PolicyErrorKind::ZeroUseLimit
            | PolicyErrorKind::SelectorOutsideBoundary
            | PolicyErrorKind::InvalidPolicyTier
            | PolicyErrorKind::InvalidRuleKind
            | PolicyErrorKind::PolicyRevisionMismatch
            | PolicyErrorKind::InvalidOperationRisk
            | PolicyErrorKind::AmendmentTierMismatch => RecoveryClass::CallerCorrectable,
        }
    }
}

} // verus!
