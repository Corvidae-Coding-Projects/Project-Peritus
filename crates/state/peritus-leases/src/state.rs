//! Privately represented aggregate state and checked immutable projections.

mod validity;

#[cfg(verus_only)]
use self::validity::{active_state_is_valid, correlation_is_valid};
use self::validity::{
    active_state_is_valid_runtime, correlation_is_valid_runtime, validity_result,
};

use crate::{
    FenceCause, LeaseClaim, LeaseError, LeaseHolder, LeasePhase, LeaseScope,
    ReconciliationCorrelation, ReconciliationDisposition,
};
use peritus_policy::{AuthorityInstant, AuthorityTimeState};
use peritus_types::{Generation, RevisionNumber};
use vstd::prelude::*;

verus! {

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ActiveLease {
    pub holder: LeaseHolder,
    pub claim_version: RevisionNumber,
    pub issued_at: AuthorityInstant,
    pub expires_at: AuthorityInstant,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReconciliationState {
    pub correlation: ReconciliationCorrelation,
    pub cause: FenceCause,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct QuarantinedState {
    pub correlation: ReconciliationCorrelation,
    pub cause: FenceCause,
    pub disposition: ReconciliationDisposition,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeaseState {
    Available,
    Active(ActiveLease),
    Reconciling(ReconciliationState),
    Quarantined(QuarantinedState),
    Retired(RetirementReason),
}

/// Permanent reason an aggregate has no representable safe successor.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RetirementReason {
    /// Fencing could not advance the workspace generation.
    GenerationExhausted,
    /// Fencing consumed the final representable aggregate version.
    VersionExhausted,
}

/// Checked immutable projection of an active lease.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ActiveLeaseView {
    claim: LeaseClaim,
}

impl ActiveLeaseView {
    pub(crate) const fn new(claim: LeaseClaim) -> Self { Self { claim } }

    /// Returns the exact unprivileged active claim.
    #[must_use]
    pub const fn claim(self) -> LeaseClaim { self.claim }
}

/// Checked immutable projection of a fenced generation awaiting reconciliation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReconciliationView {
    correlation: ReconciliationCorrelation,
    cause: FenceCause,
}

impl ReconciliationView {
    pub(crate) const fn new(
        correlation: ReconciliationCorrelation,
        cause: FenceCause,
    ) -> Self {
        Self { correlation, cause }
    }

    /// Returns the exact facts an observation must echo.
    #[must_use]
    pub const fn correlation(self) -> ReconciliationCorrelation { self.correlation }

    /// Returns the reason the prior generation was fenced.
    #[must_use]
    pub const fn cause(self) -> FenceCause { self.cause }
}

/// Checked immutable projection of quarantined reconciliation evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct QuarantinedLeaseView {
    correlation: ReconciliationCorrelation,
    cause: FenceCause,
    disposition: ReconciliationDisposition,
}

impl QuarantinedLeaseView {
    pub(crate) const fn new(
        correlation: ReconciliationCorrelation,
        cause: FenceCause,
        disposition: ReconciliationDisposition,
    ) -> Self {
        Self { correlation, cause, disposition }
    }

    /// Returns the exact fenced-lineage correlation.
    #[must_use]
    pub const fn correlation(self) -> ReconciliationCorrelation { self.correlation }

    /// Returns the original fencing cause.
    #[must_use]
    pub const fn cause(self) -> FenceCause { self.cause }

    /// Returns the dirty or indeterminate disposition.
    #[must_use]
    pub const fn disposition(self) -> ReconciliationDisposition { self.disposition }
}

/// Complete pure lease aggregate keyed by [`peritus_types::WorkspaceId`].
///
/// Fields are private and active state can be created only by accepted reducers. The aggregate is
/// move-only so its authority-time floor cannot be duplicated or reconstructed by callers.
///
/// ```compile_fail
/// use peritus_leases::LeaseAggregate;
/// fn require_clone<T: Clone>() {}
/// require_clone::<LeaseAggregate>();
/// ```
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct LeaseAggregate {
    pub(crate) scope: LeaseScope,
    pub(crate) generation: Generation,
    pub(crate) version: RevisionNumber,
    pub(crate) authority_time: AuthorityTimeState,
    pub(crate) state: LeaseState,
}

impl LeaseAggregate {
    pub(crate) const fn from_parts(
        scope: LeaseScope,
        generation: Generation,
        version: RevisionNumber,
        authority_time: AuthorityTimeState,
        state: LeaseState,
    ) -> (result: Self)
        ensures
            result.scope == scope,
            result.generation == generation,
            result.version == version,
            result.authority_time == authority_time,
            result.state == state,
    {
        Self { scope, generation, version, authority_time, state }
    }

    /// Returns the exact immutable aggregate scope.
    #[must_use]
    pub const fn scope(&self) -> LeaseScope { self.scope }

    /// Returns the current fencing generation.
    #[must_use]
    pub const fn generation(&self) -> Generation { self.generation }

    /// Returns the current aggregate compare-and-swap version.
    #[must_use]
    pub const fn version(&self) -> RevisionNumber { self.version }

    /// Returns the epoch and greatest accepted authority observation.
    #[must_use]
    pub const fn authority_time(&self) -> &AuthorityTimeState { &self.authority_time }

    /// Returns the exact private-representation phase used by crate proofs.
    pub(crate) open spec fn internal_phase(&self) -> LeasePhase {
        match self.state {
            LeaseState::Available => LeasePhase::Available,
            LeaseState::Active(_) => LeasePhase::Active,
            LeaseState::Reconciling(_) => LeasePhase::Reconciling,
            LeaseState::Quarantined(_) => LeasePhase::Quarantined,
            LeaseState::Retired(_) => LeasePhase::Retired,
        }
    }

    /// Returns the lifecycle phase used by public specifications.
    pub closed spec fn spec_phase(&self) -> LeasePhase { self.internal_phase() }

    /// Returns the public lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> (phase: LeasePhase)
        ensures phase == self.spec_phase(),
    {
        self.checked_phase()
    }

    pub(crate) const fn checked_phase(&self) -> (phase: LeasePhase)
        ensures phase == self.internal_phase(),
    {
        match self.state {
            LeaseState::Available => LeasePhase::Available,
            LeaseState::Active(_) => LeasePhase::Active,
            LeaseState::Reconciling(_) => LeasePhase::Reconciling,
            LeaseState::Quarantined(_) => LeasePhase::Quarantined,
            LeaseState::Retired(_) => LeasePhase::Retired,
        }
    }

    /// Returns the active projection, if this aggregate has one holder.
    #[must_use]
    pub const fn active(&self) -> Option<ActiveLeaseView> {
        match self.state {
            LeaseState::Active(active) => Some(ActiveLeaseView::new(LeaseClaim::new(
                self.scope,
                active.holder,
                self.generation,
                active.claim_version,
                active.issued_at,
                active.expires_at,
            ))),
            _ => None,
        }
    }

    /// Returns the pending reconciliation projection.
    #[must_use]
    pub const fn reconciliation(&self) -> Option<ReconciliationView> {
        match self.state {
            LeaseState::Reconciling(reconciling) => Some(ReconciliationView::new(
                reconciling.correlation,
                reconciling.cause,
            )),
            _ => None,
        }
    }

    /// Returns the quarantined projection.
    #[must_use]
    pub const fn quarantine(&self) -> Option<QuarantinedLeaseView> {
        match self.state {
            LeaseState::Quarantined(quarantined) => Some(QuarantinedLeaseView::new(
                quarantined.correlation,
                quarantined.cause,
                quarantined.disposition,
            )),
            _ => None,
        }
    }

    /// Returns the permanent retirement reason.
    #[must_use]
    pub const fn retirement_reason(&self) -> Option<RetirementReason> {
        match self.state {
            LeaseState::Retired(reason) => Some(reason),
            _ => None,
        }
    }

    /// Exact validity predicate for every private aggregate phase relation.
    pub(crate) open spec fn internal_is_valid(&self) -> bool {
        match self.state {
            LeaseState::Available | LeaseState::Retired(_) => true,
            LeaseState::Active(active) => active_state_is_valid(self, active),
            LeaseState::Reconciling(reconciling) => {
                correlation_is_valid(self, reconciling.correlation)
            }
            LeaseState::Quarantined(quarantined) => {
                !matches!(
                    quarantined.disposition,
                    ReconciliationDisposition::SafeToAcquire { .. }
                ) && correlation_is_valid(self, quarantined.correlation)
            }
        }
    }

    /// Returns the complete private-representation validity relation.
    pub closed spec fn spec_is_valid(&self) -> bool {
        self.internal_is_valid()
    }

    pub(crate) proof fn reveal_internal_validity(&self)
        ensures self.spec_is_valid() == self.internal_is_valid(),
    {
    }

    /// Validates every internal phase relation for checked replay boundaries.
    ///
    /// # Errors
    ///
    /// Returns [`LeaseError::CorruptState`] for an impossible phase/time/generation relation.
    pub const fn validate(&self) -> (result: Result<(), LeaseError>)
        ensures
            match result {
                Ok(()) => self.spec_is_valid(),
                Err(error) => {
                    !self.spec_is_valid() && error == LeaseError::CorruptState
                }
            },
    {
        match self.state {
            LeaseState::Available | LeaseState::Retired(_) => {
                assert(self.internal_is_valid());
                assert(self.spec_is_valid());
                validity_result(true)
            }
            LeaseState::Active(active) => {
                let valid = active_state_is_valid_runtime(self, active);
                proof {
                    assert(self.internal_is_valid() == valid);
                    assert(self.spec_is_valid() == self.internal_is_valid());
                }
                validity_result(valid)
            }
            LeaseState::Reconciling(reconciling) => {
                let valid = correlation_is_valid_runtime(self, reconciling.correlation);
                proof {
                    assert(self.internal_is_valid()
                        == valid);
                    assert(self.spec_is_valid() == self.internal_is_valid());
                }
                validity_result(valid)
            }
            LeaseState::Quarantined(quarantined) => {
                let unsafe_disposition = !matches!(
                    quarantined.disposition,
                    ReconciliationDisposition::SafeToAcquire { .. },
                );
                let correlation_valid = correlation_is_valid_runtime(
                    self,
                    quarantined.correlation,
                );
                let valid = unsafe_disposition && correlation_valid;
                proof {
                    assert(self.internal_is_valid() == valid);
                    assert(self.spec_is_valid() == self.internal_is_valid());
                }
                validity_result(valid)
            }
        }
    }
}

} // verus!
