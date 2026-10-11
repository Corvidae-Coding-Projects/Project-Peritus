//! Stable fail-closed failures and recovery guidance.
#![allow(
    missing_docs,
    reason = "pinned Cargo-Verus synthesizes undocumented accessors for documented payload variants"
)]

#[doc = " Public lifecycle phase used in diagnostics and projections."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeasePhase {
    #[doc = " No current holder exists and acquisition may be planned."]
    Available,
    #[doc = " One exact holder owns the current logical lease."]
    Active,
    #[doc = " The prior generation is fenced while safety is re-established."]
    Reconciling,
    #[doc = " Evidence reported dirty or indeterminate resource state."]
    Quarantined,
    #[doc = " The aggregate can never transition again."]
    Retired,
}
#[doc = " Recovery guidance attached to every lease failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    #[doc = " The aggregate or request is permanently unusable without changed intent."]
    Terminal,
    #[doc = " Current state must be observed and the command planned again."]
    Reobserve,
    #[doc = " Policy must be evaluated and consumed again against current state."]
    Reauthorize,
    #[doc = " The same command identity must be resolved before any new attempt."]
    ResolveIndeterminate,
    #[doc = " The caller may correct malformed or mismatched evidence."]
    CallerCorrectable,
}
#[doc = " One exact lease-scope dimension."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScopeDimension {
    #[doc = " Workspace lineage and aggregate key."]
    Workspace,
    #[doc = " Resolved mutation resource."]
    Resource,
    #[doc = " Execution environment."]
    Environment,
}
#[doc = " One exact reconciliation-correlation dimension."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReconciliationDimension {
    #[doc = " Lease scope."]
    Scope(ScopeDimension),
    #[doc = " Generation that was fenced."]
    FencedGeneration,
    #[doc = " Prior actor/session holder."]
    PriorHolder,
}
#[doc = " One exact policy/lease intersection dimension."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PolicyIntersectionDimension {
    #[doc = " Actor identity."]
    Actor,
    #[doc = " Execution environment."]
    Environment,
    #[doc = " Workspace lineage embedded in the revision tuple."]
    Workspace,
    #[doc = " Workspace generation embedded in the revision tuple."]
    Generation,
    #[doc = " Exact resolved resource permission."]
    ResourcePermission,
    #[doc = " Authority-clock epoch."]
    ClockEpoch,
    #[doc = " Action identity."]
    Action,
}
#[doc = " Checked-constructor, reducer, replay, and observation failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeaseError {
    #[doc = " A lease duration was zero."]
    ZeroDuration,
    #[doc = " Authority-time arithmetic overflowed."]
    TimeOverflow,
    #[doc = " A same-epoch authority observation regressed."]
    ClockRegression,
    #[doc = " Ordinary processing observed a different authority-clock epoch."]
    ClockEpochMismatch,
    #[doc = " An explicit discontinuity command did not describe a discontinuity."]
    NoClockDiscontinuity,
    #[doc = " The aggregate was not in the phase required by the command."]
    IllegalPhase {
        #[doc = " Phase required by the attempted command."]
        expected: LeasePhase,
        #[doc = " Phase actually observed in the immutable input."]
        actual: LeasePhase,
    },
    #[doc = " A claim did not match one exact scope dimension."]
    ClaimScopeMismatch(ScopeDimension),
    #[doc = " A claim named another actor/session holder."]
    ClaimHolderMismatch,
    #[doc = " A claim belonged to an old or future generation."]
    ClaimGenerationMismatch,
    #[doc = " A claim version was superseded by renewal or use."]
    ClaimVersionMismatch,
    #[doc = " The claim was used at or after its lease expiry."]
    ClaimExpired,
    #[doc = " Renewal did not strictly extend the current deadline."]
    DeadlineNotExtended,
    #[doc = " Expiry was requested before the exact deadline."]
    LeaseNotExpired,
    #[doc = " Holder-loss evidence did not match the active claim."]
    HolderLossMismatch,
    #[doc = " Quiescence evidence did not match the active claim."]
    HolderQuiescenceMismatch,
    #[doc = " Reconciliation evidence did not match the fenced lineage."]
    ReconciliationMismatch(ReconciliationDimension),
    #[doc = " The generation cannot be advanced; fencing retires instead."]
    GenerationExhausted,
    #[doc = " Aggregate version space is exhausted or its reserved fence step would be lost."]
    VersionExhausted,
    #[doc = " Claim-version space is exhausted."]
    ClaimVersionExhausted,
    #[doc = " A capability-use transition did not exactly intersect the lease."]
    PolicyIntersectionMismatch(PolicyIntersectionDimension),
    #[doc = " A capability-use transition was invalid at the lease observation."]
    PolicyUseInvalid,
    #[doc = " Checked replay found an impossible aggregate shape."]
    CorruptState,
}
impl LeaseError {
    #[doc = " Returns the stable subsystem diagnostic code."]
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
    #[doc = " Returns the recovery class for this failure."]
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        match self {
            Self::ClockRegression
            | Self::ClockEpochMismatch
            | Self::LeaseNotExpired
            | Self::IllegalPhase { .. }
            | Self::ClaimGenerationMismatch
            | Self::ClaimVersionMismatch => RecoveryClass::Reobserve,
            Self::ClaimExpired | Self::PolicyIntersectionMismatch(_) | Self::PolicyUseInvalid => {
                RecoveryClass::Reauthorize
            }
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
#[doc = " Rejected move-only lease reducer result that preserves the unchanged aggregate."]
#[doc = ""]
#[doc = " The failure owns the original authority-time floor, so a rejected command cannot duplicate,"]
#[doc = " discard, reconstruct, or advance that floor. Consuming the failure is the only way to recover"]
#[doc = " the aggregate for a corrected command."]
#[derive(Debug, Eq, PartialEq)]
pub struct LeaseTransitionFailure {
    error: LeaseError,
    aggregate: crate::LeaseAggregate,
}
impl LeaseTransitionFailure {
    pub(crate) const fn new(error: LeaseError, aggregate: crate::LeaseAggregate) -> Self {
        Self { error, aggregate }
    }
    #[doc = " Borrows the typed rejection reason."]
    #[must_use]
    pub const fn error(&self) -> &LeaseError {
        &self.error
    }
    #[doc = " Borrows the unchanged aggregate, including its linear authority-time floor."]
    #[must_use]
    pub const fn aggregate(&self) -> &crate::LeaseAggregate {
        &self.aggregate
    }
    #[doc = " Consumes the failure and returns the unchanged aggregate."]
    #[must_use]
    pub const fn into_aggregate(self) -> crate::LeaseAggregate {
        self.aggregate
    }
    #[doc = " Consumes the failure into its typed reason and unchanged aggregate."]
    #[must_use]
    pub const fn into_parts(self) -> (LeaseError, crate::LeaseAggregate) {
        (self.error, self.aggregate)
    }
}
#[doc = " Rejected lease/policy intersection that preserves both linear inputs."]
#[doc = ""]
#[doc = " The failure owns the unchanged lease aggregate and the still-unprivileged policy-use command."]
#[doc = " Callers may correct an aggregate selection or inspect and retire the exact attempted command"]
#[doc = " without cloning either input."]
pub struct LeaseUseFailure {
    lease: LeaseTransitionFailure,
    command: crate::UseLease,
}
impl LeaseUseFailure {
    pub(crate) const fn new(lease: LeaseTransitionFailure, command: crate::UseLease) -> Self {
        Self { lease, command }
    }
    #[doc = " Borrows the typed lease-intersection rejection reason."]
    #[must_use]
    pub const fn error(&self) -> &LeaseError {
        self.lease.error()
    }
    #[doc = " Borrows the unchanged aggregate and its linear authority-time floor."]
    #[must_use]
    pub const fn aggregate(&self) -> &crate::LeaseAggregate {
        self.lease.aggregate()
    }
    #[doc = " Borrows the exact rejected policy-use command."]
    #[must_use]
    pub const fn command(&self) -> &crate::UseLease {
        &self.command
    }
    #[doc = " Consumes the failure into the preserving lease failure and rejected command."]
    #[must_use]
    pub fn into_parts(self) -> (LeaseTransitionFailure, crate::UseLease) {
        (self.lease, self.command)
    }
}
