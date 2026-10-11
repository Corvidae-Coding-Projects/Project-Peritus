//! Privately represented aggregate state and checked immutable projections.

mod validity;

use self::validity::{
    active_state_is_valid_runtime, correlation_is_valid_runtime, validity_result,
};

use crate::{
    FenceCause, LeaseClaim, LeaseError, LeaseHolder, LeasePhase, LeaseScope,
    ReconciliationCorrelation, ReconciliationDisposition,
};
use peritus_policy::{AuthorityInstant, AuthorityTimeState};
use peritus_types::{Generation, RevisionNumber};

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
#[doc = " Permanent reason an aggregate has no representable safe successor."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RetirementReason {
    #[doc = " Fencing could not advance the workspace generation."]
    GenerationExhausted,
    #[doc = " Fencing consumed the final representable aggregate version."]
    VersionExhausted,
}
#[doc = " Checked immutable projection of an active lease."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ActiveLeaseView {
    claim: LeaseClaim,
}
impl ActiveLeaseView {
    pub(crate) const fn new(claim: LeaseClaim) -> Self {
        Self { claim }
    }
    #[doc = " Returns the exact unprivileged active claim."]
    #[must_use]
    pub const fn claim(self) -> LeaseClaim {
        self.claim
    }
}
#[doc = " Checked immutable projection of a fenced generation awaiting reconciliation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReconciliationView {
    correlation: ReconciliationCorrelation,
    cause: FenceCause,
}
impl ReconciliationView {
    pub(crate) const fn new(correlation: ReconciliationCorrelation, cause: FenceCause) -> Self {
        Self { correlation, cause }
    }
    #[doc = " Returns the exact facts an observation must echo."]
    #[must_use]
    pub const fn correlation(self) -> ReconciliationCorrelation {
        self.correlation
    }
    #[doc = " Returns the reason the prior generation was fenced."]
    #[must_use]
    pub const fn cause(self) -> FenceCause {
        self.cause
    }
}
#[doc = " Checked immutable projection of quarantined reconciliation evidence."]
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
    #[doc = " Returns the exact fenced-lineage correlation."]
    #[must_use]
    pub const fn correlation(self) -> ReconciliationCorrelation {
        self.correlation
    }
    #[doc = " Returns the original fencing cause."]
    #[must_use]
    pub const fn cause(self) -> FenceCause {
        self.cause
    }
    #[doc = " Returns the dirty or indeterminate disposition."]
    #[must_use]
    pub const fn disposition(self) -> ReconciliationDisposition {
        self.disposition
    }
}
#[doc = " Complete pure lease aggregate keyed by [`peritus_types::WorkspaceId`]."]
#[doc = ""]
#[doc = " Fields are private and active state can be created only by accepted reducers. The aggregate is"]
#[doc = " move-only so its authority-time floor cannot be duplicated or reconstructed by callers."]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " use peritus_leases::LeaseAggregate;"]
#[doc = " fn require_clone<T: Clone>() {}"]
#[doc = " require_clone::<LeaseAggregate>();"]
#[doc = " ```"]
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
    ) -> Self {
        Self { scope, generation, version, authority_time, state }
    }
    #[doc = " Returns the exact immutable aggregate scope."]
    #[must_use]
    pub const fn scope(&self) -> LeaseScope {
        self.scope
    }
    #[doc = " Returns the current fencing generation."]
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }
    #[doc = " Returns the current aggregate compare-and-swap version."]
    #[must_use]
    pub const fn version(&self) -> RevisionNumber {
        self.version
    }
    #[doc = " Returns the epoch and greatest accepted authority observation."]
    #[must_use]
    pub const fn authority_time(&self) -> &AuthorityTimeState {
        &self.authority_time
    }
    #[doc = " Returns the public lifecycle phase."]
    #[must_use]
    pub const fn phase(&self) -> LeasePhase {
        self.checked_phase()
    }
    pub(crate) const fn checked_phase(&self) -> LeasePhase {
        match self.state {
            LeaseState::Available => LeasePhase::Available,
            LeaseState::Active(_) => LeasePhase::Active,
            LeaseState::Reconciling(_) => LeasePhase::Reconciling,
            LeaseState::Quarantined(_) => LeasePhase::Quarantined,
            LeaseState::Retired(_) => LeasePhase::Retired,
        }
    }
    #[doc = " Returns the active projection, if this aggregate has one holder."]
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
    #[doc = " Returns the pending reconciliation projection."]
    #[must_use]
    pub const fn reconciliation(&self) -> Option<ReconciliationView> {
        match self.state {
            LeaseState::Reconciling(reconciling) => {
                Some(ReconciliationView::new(reconciling.correlation, reconciling.cause))
            }
            _ => None,
        }
    }
    #[doc = " Returns the quarantined projection."]
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
    #[doc = " Returns the permanent retirement reason."]
    #[must_use]
    pub const fn retirement_reason(&self) -> Option<RetirementReason> {
        match self.state {
            LeaseState::Retired(reason) => Some(reason),
            _ => None,
        }
    }
    #[doc = " Validates every internal phase relation for checked replay boundaries."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`LeaseError::CorruptState`] for an impossible phase/time/generation relation."]
    pub const fn validate(&self) -> Result<(), LeaseError> {
        match self.state {
            LeaseState::Available | LeaseState::Retired(_) => validity_result(true),
            LeaseState::Active(active) => {
                let valid = active_state_is_valid_runtime(self, active);
                validity_result(valid)
            }
            LeaseState::Reconciling(reconciling) => {
                let valid = correlation_is_valid_runtime(self, reconciling.correlation);
                validity_result(valid)
            }
            LeaseState::Quarantined(quarantined) => {
                let unsafe_disposition = !matches!(
                    quarantined.disposition,
                    ReconciliationDisposition::SafeToAcquire { .. },
                );
                let correlation_valid = correlation_is_valid_runtime(self, quarantined.correlation);
                let valid = unsafe_disposition && correlation_valid;
                validity_result(valid)
            }
        }
    }
}
