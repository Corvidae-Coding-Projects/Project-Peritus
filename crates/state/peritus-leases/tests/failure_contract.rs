//! Exhaustive stable diagnostics and recovery classifications for lease-domain failures.

use peritus_leases::{
    LeaseError, LeasePhase, PolicyIntersectionDimension, ReconciliationDimension, RecoveryClass,
    ScopeDimension,
};

#[test]
fn every_lease_error_variant_has_a_stable_code_and_recovery_class() {
    let cases = [
        (LeaseError::ZeroDuration, "PERITUS-LEASE-INPUT-001", RecoveryClass::CallerCorrectable),
        (LeaseError::TimeOverflow, "PERITUS-LEASE-TIME-001", RecoveryClass::Terminal),
        (LeaseError::ClockRegression, "PERITUS-LEASE-TIME-002", RecoveryClass::Reobserve),
        (LeaseError::ClockEpochMismatch, "PERITUS-LEASE-TIME-003", RecoveryClass::Reobserve),
        (
            LeaseError::NoClockDiscontinuity,
            "PERITUS-LEASE-TIME-004",
            RecoveryClass::CallerCorrectable,
        ),
        (
            LeaseError::IllegalPhase {
                expected: LeasePhase::Active,
                actual: LeasePhase::Available,
            },
            "PERITUS-LEASE-STATE-001",
            RecoveryClass::Reobserve,
        ),
        (
            LeaseError::ClaimScopeMismatch(ScopeDimension::Workspace),
            "PERITUS-LEASE-CLAIM-001",
            RecoveryClass::CallerCorrectable,
        ),
        (
            LeaseError::ClaimHolderMismatch,
            "PERITUS-LEASE-CLAIM-002",
            RecoveryClass::CallerCorrectable,
        ),
        (LeaseError::ClaimGenerationMismatch, "PERITUS-LEASE-CLAIM-003", RecoveryClass::Reobserve),
        (LeaseError::ClaimVersionMismatch, "PERITUS-LEASE-CLAIM-004", RecoveryClass::Reobserve),
        (LeaseError::ClaimExpired, "PERITUS-LEASE-CLAIM-005", RecoveryClass::Reauthorize),
        (
            LeaseError::DeadlineNotExtended,
            "PERITUS-LEASE-RENEW-001",
            RecoveryClass::CallerCorrectable,
        ),
        (LeaseError::LeaseNotExpired, "PERITUS-LEASE-EXPIRE-001", RecoveryClass::Reobserve),
        (
            LeaseError::HolderLossMismatch,
            "PERITUS-LEASE-FENCE-001",
            RecoveryClass::CallerCorrectable,
        ),
        (
            LeaseError::HolderQuiescenceMismatch,
            "PERITUS-LEASE-FENCE-002",
            RecoveryClass::CallerCorrectable,
        ),
        (
            LeaseError::ReconciliationMismatch(ReconciliationDimension::FencedGeneration),
            "PERITUS-LEASE-RECONCILE-001",
            RecoveryClass::CallerCorrectable,
        ),
        (LeaseError::GenerationExhausted, "PERITUS-LEASE-GENERATION-001", RecoveryClass::Terminal),
        (LeaseError::VersionExhausted, "PERITUS-LEASE-VERSION-001", RecoveryClass::Terminal),
        (LeaseError::ClaimVersionExhausted, "PERITUS-LEASE-VERSION-002", RecoveryClass::Terminal),
        (
            LeaseError::PolicyIntersectionMismatch(PolicyIntersectionDimension::Actor),
            "PERITUS-LEASE-AUTHORITY-001",
            RecoveryClass::Reauthorize,
        ),
        (LeaseError::PolicyUseInvalid, "PERITUS-LEASE-AUTHORITY-002", RecoveryClass::Reauthorize),
        (LeaseError::CorruptState, "PERITUS-LEASE-STATE-002", RecoveryClass::Terminal),
    ];

    for (error, code, recovery) in cases {
        assert_eq!(error.code(), code, "{error:?}");
        assert_eq!(error.recovery(), recovery, "{error:?}");
    }
}
