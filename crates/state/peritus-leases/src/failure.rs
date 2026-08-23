//! Stable fail-closed failures and recovery guidance.
#![allow(
    missing_docs,
    reason = "pinned Cargo-Verus synthesizes undocumented accessors for documented payload variants"
)]

use vstd::prelude::*;

verus! {

/// Public lifecycle phase used in diagnostics and projections.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeasePhase {
    /// No current holder exists and acquisition may be planned.
    Available,
    /// One exact holder owns the current logical lease.
    Active,
    /// The prior generation is fenced while safety is re-established.
    Reconciling,
    /// Evidence reported dirty or indeterminate resource state.
    Quarantined,
    /// The aggregate can never transition again.
    Retired,
}

/// Recovery guidance attached to every lease failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// The aggregate or request is permanently unusable without changed intent.
    Terminal,
    /// Current state must be observed and the command planned again.
    Reobserve,
    /// Policy must be evaluated and consumed again against current state.
    Reauthorize,
    /// The same command identity must be resolved before any new attempt.
    ResolveIndeterminate,
    /// The caller may correct malformed or mismatched evidence.
    CallerCorrectable,
}

/// One exact lease-scope dimension.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScopeDimension {
    /// Workspace lineage and aggregate key.
    Workspace,
    /// Resolved mutation resource.
    Resource,
    /// Execution environment.
    Environment,
}

/// One exact reconciliation-correlation dimension.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReconciliationDimension {
    /// Lease scope.
    Scope(ScopeDimension),
    /// Generation that was fenced.
    FencedGeneration,
    /// Prior actor/session holder.
    PriorHolder,
}

/// One exact policy/lease intersection dimension.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PolicyIntersectionDimension {
    /// Actor identity.
    Actor,
    /// Execution environment.
    Environment,
    /// Workspace lineage embedded in the revision tuple.
    Workspace,
    /// Workspace generation embedded in the revision tuple.
    Generation,
    /// Exact resolved resource permission.
    ResourcePermission,
    /// Authority-clock epoch.
    ClockEpoch,
    /// Action identity.
    Action,
}

/// Checked-constructor, reducer, replay, and observation failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeaseError {
    /// A lease duration was zero.
    ZeroDuration,
    /// Authority-time arithmetic overflowed.
    TimeOverflow,
    /// A same-epoch authority observation regressed.
    ClockRegression,
    /// Ordinary processing observed a different authority-clock epoch.
    ClockEpochMismatch,
    /// An explicit discontinuity command did not describe a discontinuity.
    NoClockDiscontinuity,
    /// The aggregate was not in the phase required by the command.
    IllegalPhase {
        /// Phase required by the attempted command.
        expected: LeasePhase,
        /// Phase actually observed in the immutable input.
        actual: LeasePhase,
    },
    /// A claim did not match one exact scope dimension.
    ClaimScopeMismatch(ScopeDimension),
    /// A claim named another actor/session holder.
    ClaimHolderMismatch,
    /// A claim belonged to an old or future generation.
    ClaimGenerationMismatch,
    /// A claim version was superseded by renewal or use.
    ClaimVersionMismatch,
    /// The claim was used at or after its lease expiry.
    ClaimExpired,
    /// Renewal did not strictly extend the current deadline.
    DeadlineNotExtended,
    /// Expiry was requested before the exact deadline.
    LeaseNotExpired,
    /// Holder-loss evidence did not match the active claim.
    HolderLossMismatch,
    /// Quiescence evidence did not match the active claim.
    HolderQuiescenceMismatch,
    /// Reconciliation evidence did not match the fenced lineage.
    ReconciliationMismatch(ReconciliationDimension),
    /// The generation cannot be advanced; fencing retires instead.
    GenerationExhausted,
    /// Aggregate version space is exhausted or its reserved fence step would be lost.
    VersionExhausted,
    /// Claim-version space is exhausted.
    ClaimVersionExhausted,
    /// A capability-use transition did not exactly intersect the lease.
    PolicyIntersectionMismatch(PolicyIntersectionDimension),
    /// A capability-use transition was invalid at the lease observation.
    PolicyUseInvalid,
    /// Checked replay found an impossible aggregate shape.
    CorruptState,
}

impl LeaseError {
    /// Returns the stable subsystem diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ZeroDuration => "PERITUS-LEASE-INPUT-001",
            Self::TimeOverflow => "PERITUS-LEASE-TIME-001",
            Self::ClockRegression => "PERITUS-LEASE-TIME-002",
            Self::ClockEpochMismatch => "PERITUS-LEASE-TIME-003",
            Self::NoClockDiscontinuity => "PERITUS-LEASE-TIME-004",
            Self::IllegalPhase { .. } => "PERITUS-LEASE-STATE-001",
            Self::ClaimScopeMismatch(_) => "PERITUS-LEASE-CLAIM-001",
            Self::ClaimHolderMismatch => "PERITUS-LEASE-CLAIM-002",
            Self::ClaimGenerationMismatch => "PERITUS-LEASE-CLAIM-003",
            Self::ClaimVersionMismatch => "PERITUS-LEASE-CLAIM-004",
            Self::ClaimExpired => "PERITUS-LEASE-CLAIM-005",
            Self::DeadlineNotExtended => "PERITUS-LEASE-RENEW-001",
            Self::LeaseNotExpired => "PERITUS-LEASE-EXPIRE-001",
            Self::HolderLossMismatch => "PERITUS-LEASE-FENCE-001",
            Self::HolderQuiescenceMismatch => "PERITUS-LEASE-FENCE-002",
            Self::ReconciliationMismatch(_) => "PERITUS-LEASE-RECONCILE-001",
            Self::GenerationExhausted => "PERITUS-LEASE-GENERATION-001",
            Self::VersionExhausted => "PERITUS-LEASE-VERSION-001",
            Self::ClaimVersionExhausted => "PERITUS-LEASE-VERSION-002",
            Self::PolicyIntersectionMismatch(_) => "PERITUS-LEASE-AUTHORITY-001",
            Self::PolicyUseInvalid => "PERITUS-LEASE-AUTHORITY-002",
            Self::CorruptState => "PERITUS-LEASE-STATE-002",
        }
    }

    /// Returns the recovery class for this failure.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        match self {
            Self::ClockRegression
            | Self::ClockEpochMismatch
            | Self::LeaseNotExpired
            | Self::IllegalPhase { .. }
            | Self::ClaimGenerationMismatch
            | Self::ClaimVersionMismatch => RecoveryClass::Reobserve,
            Self::ClaimExpired
            | Self::PolicyIntersectionMismatch(_)
            | Self::PolicyUseInvalid => RecoveryClass::Reauthorize,
            Self::ZeroDuration
            | Self::NoClockDiscontinuity
            | Self::ClaimScopeMismatch(_)
            | Self::ClaimHolderMismatch
            | Self::HolderLossMismatch
            | Self::HolderQuiescenceMismatch
            | Self::ReconciliationMismatch(_)
            | Self::DeadlineNotExtended => RecoveryClass::CallerCorrectable,
            Self::TimeOverflow
            | Self::GenerationExhausted
            | Self::VersionExhausted
            | Self::ClaimVersionExhausted
            | Self::CorruptState => RecoveryClass::Terminal,
        }
    }
}

/// Rejected move-only lease reducer result that preserves the unchanged aggregate.
///
/// The failure owns the original authority-time floor, so a rejected command cannot duplicate,
/// discard, reconstruct, or advance that floor. Consuming the failure is the only way to recover
/// the aggregate for a corrected command.
#[derive(Debug, Eq, PartialEq)]
pub struct LeaseTransitionFailure {
    error: LeaseError,
    aggregate: crate::LeaseAggregate,
}

impl LeaseTransitionFailure {
    pub closed spec fn spec_error(&self) -> LeaseError {
        self.error
    }

    pub closed spec fn spec_aggregate(&self) -> crate::LeaseAggregate {
        self.aggregate
    }

    pub(crate) const fn new(
        error: LeaseError,
        aggregate: crate::LeaseAggregate,
    ) -> (failure: Self)
        ensures
            failure.spec_error() == error,
            failure.spec_aggregate() == aggregate,
    {
        Self { error, aggregate }
    }

    /// Borrows the typed rejection reason.
    #[must_use]
    pub const fn error(&self) -> (error: &LeaseError)
        ensures *error == self.spec_error(),
    { &self.error }

    /// Borrows the unchanged aggregate, including its linear authority-time floor.
    #[must_use]
    pub const fn aggregate(&self) -> (aggregate: &crate::LeaseAggregate)
        ensures *aggregate == self.spec_aggregate(),
    { &self.aggregate }

    /// Consumes the failure and returns the unchanged aggregate.
    #[must_use]
    pub const fn into_aggregate(self) -> crate::LeaseAggregate { self.aggregate }

    /// Consumes the failure into its typed reason and unchanged aggregate.
    #[must_use]
    pub const fn into_parts(self) -> (LeaseError, crate::LeaseAggregate) {
        (self.error, self.aggregate)
    }
}

/// Rejected lease/policy intersection that preserves both linear inputs.
///
/// The failure owns the unchanged lease aggregate and the still-unprivileged policy-use command.
/// Callers may correct an aggregate selection or inspect and retire the exact attempted command
/// without cloning either input.
pub struct LeaseUseFailure {
    lease: LeaseTransitionFailure,
    command: crate::UseLease,
}

impl LeaseUseFailure {
    pub(crate) closed spec fn spec_error(&self) -> LeaseError {
        self.lease.spec_error()
    }

    pub(crate) closed spec fn spec_aggregate(&self) -> crate::LeaseAggregate {
        self.lease.spec_aggregate()
    }

    pub(crate) closed spec fn spec_command(&self) -> crate::UseLease {
        self.command
    }

    pub(crate) const fn new(
        lease: LeaseTransitionFailure,
        command: crate::UseLease,
    ) -> (failure: Self)
        ensures
            failure.spec_error() == lease.spec_error(),
            failure.spec_aggregate() == lease.spec_aggregate(),
            failure.spec_command() == command,
    {
        Self { lease, command }
    }

    /// Borrows the typed lease-intersection rejection reason.
    #[must_use]
    pub const fn error(&self) -> &LeaseError { self.lease.error() }

    /// Borrows the unchanged aggregate and its linear authority-time floor.
    #[must_use]
    pub const fn aggregate(&self) -> &crate::LeaseAggregate { self.lease.aggregate() }

    /// Borrows the exact rejected policy-use command.
    #[must_use]
    pub const fn command(&self) -> &crate::UseLease { &self.command }

    /// Consumes the failure into the preserving lease failure and rejected command.
    #[must_use]
    pub fn into_parts(self) -> (LeaseTransitionFailure, crate::UseLease) {
        (self.lease, self.command)
    }
}

} // verus!
