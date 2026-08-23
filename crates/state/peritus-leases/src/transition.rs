//! Pure checked reducers and move-only logical transition values.
#![allow(
    missing_docs,
    reason = "pinned Cargo-Verus synthesizes undocumented accessors for documented payload variants"
)]

mod authority;
mod authority_validation;
#[cfg(test)]
mod boundary_tests;
#[cfg(test)]
mod boundary_reference;
#[cfg(test)]
mod retired_tests;
mod core;
mod core_proofs;
mod fencing;
mod fencing_apply;
mod fencing_rejections;
mod fencing_model;
mod fencing_retire;
mod fencing_validation;
mod lifecycle;
mod reconciliation;
mod record_duplication;
mod validation;

use self::validation::{
    earlier, ensure_before_expiry, map_policy_time, next_active_version,
    next_non_fence_version, require_active, require_active_claim, require_phase,
    validate_observation,
};
#[cfg(verus_only)]
pub(crate) use self::validation::{
    active_claim_error, active_error, before_expiry_error, observation_error, phase_error,
};
#[cfg(verus_only)]
pub(crate) use self::reconciliation::{correlation_error, reconciliation_time_error};
use self::core::{rejection, transition, AuthorityTimeAdvance, TransitionPlan};
use crate::state::LeaseState;
use crate::{
    LeaseAggregate, LeaseClaim, LeaseError, LeasePhase, LeaseTransitionFailure, RetirementReason,
};
use peritus_policy::{AuthorityInstant, CapabilityUseTransition};
use peritus_types::{ActionId, CommandId, Generation, RevisionNumber, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Semantic kind carried by every compare-and-swap plan.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeaseTransitionKind {
    /// Established a previously absent aggregate.
    Minted,
    /// Acquired an available generation.
    Acquired,
    /// Strictly extended an active claim.
    Renewed,
    /// Consumed a logical lease/policy intersection for one action.
    Used {
        /// Exact action identity.
        action_id: ActionId,
        /// Exact action digest bound by policy.
        action_digest: Sha256Digest,
    },
    /// Released and fenced with exact quiescence evidence.
    ReleasedAvailable,
    /// Released and fenced pending reconciliation.
    ReleasedReconciling,
    /// Expired and fenced pending reconciliation.
    Expired,
    /// Fenced after exact holder-loss evidence.
    HolderLost,
    /// Fenced across an explicit clock discontinuity.
    ClockDiscontinuity,
    /// Fenced after a separately authorized revocation.
    Revoked,
    /// Accepted safe correlated evidence and became available.
    ReconciledAvailable,
    /// Accepted dirty or indeterminate evidence and quarantined.
    ReconciledQuarantined,
    /// Fencing retired the aggregate.
    Retired(RetirementReason),
}

/// Unprivileged exact typed record for one accepted logical state edge.
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct LeaseTransitionRecord {
    pub(crate) command_id: CommandId,
    pub(crate) scope: crate::LeaseScope,
    pub(crate) before_version: Option<RevisionNumber>,
    pub(crate) after_version: RevisionNumber,
    pub(crate) before_generation: Option<Generation>,
    pub(crate) after_generation: Generation,
    pub(crate) before_phase: Option<LeasePhase>,
    pub(crate) after_phase: LeasePhase,
    pub(crate) kind: LeaseTransitionKind,
    pub(crate) binding: Box<crate::LeaseCommandBinding>,
}

impl LeaseTransitionRecord {
    /// Returns the idempotency identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId { self.command_id }
    /// Returns the exact aggregate scope.
    #[must_use]
    pub const fn scope(&self) -> crate::LeaseScope { self.scope }
    /// Returns the expected prior version, or absence for mint.
    #[must_use]
    pub const fn before_version(&self) -> Option<RevisionNumber> { self.before_version }
    /// Returns the planned successor version.
    #[must_use]
    pub const fn after_version(&self) -> RevisionNumber { self.after_version }
    /// Returns the prior generation, or absence for mint.
    #[must_use]
    pub const fn before_generation(&self) -> Option<Generation> { self.before_generation }
    /// Returns the successor generation.
    #[must_use]
    pub const fn after_generation(&self) -> Generation { self.after_generation }
    /// Returns the prior phase, or absence for mint.
    #[must_use]
    pub const fn before_phase(&self) -> Option<LeasePhase> { self.before_phase }
    /// Returns the successor phase.
    #[must_use]
    pub const fn after_phase(&self) -> LeasePhase { self.after_phase }
    /// Returns the semantic edge kind.
    #[must_use]
    pub const fn kind(&self) -> LeaseTransitionKind { self.kind }
    /// Returns the exact unprivileged source-command projection.
    #[must_use]
    pub fn binding(&self) -> &crate::LeaseCommandBinding { &self.binding }
}

/// Move-only pure transition plan. It does not prove persistence or authorize an effect.
///
/// ```compile_fail
/// use peritus_leases::LeaseTransition;
/// fn require_clone<T: Clone>() {}
/// require_clone::<LeaseTransition>();
/// ```
pub struct LeaseTransition {
    pub(crate) next: LeaseAggregate,
    pub(crate) record: LeaseTransitionRecord,
}

/// Move-only result of a reducer that must preserve its linear input on rejection.
///
/// Both variants own their complete result. The rejected variant carries the exact unchanged
/// aggregate and its authority-time floor through [`LeaseTransitionFailure`].
#[must_use = "the accepted transition or preserving rejection must be consumed"]
pub enum LeaseTransitionOutcome {
    /// The command produced one accepted logical transition.
    Accepted(LeaseTransition),
    /// The command was rejected and preserved its input aggregate.
    Rejected(LeaseTransitionFailure),
}

/// Move-only result of exact policy-use intersection with a linear lease aggregate.
#[must_use = "the accepted logical use or preserving rejection must be consumed"]
pub enum LeaseUseOutcome {
    /// The command produced one accepted logical lease/policy intersection.
    Accepted(LeaseUseTransition),
    /// The command was rejected and preserved its input aggregate.
    Rejected(crate::LeaseUseFailure),
}

impl LeaseTransition {
    /// Borrows the planned successor snapshot.
    #[must_use]
    pub const fn next(&self) -> &LeaseAggregate { &self.next }
    /// Returns the unprivileged typed state-edge record.
    #[must_use]
    pub const fn record(&self) -> &LeaseTransitionRecord { &self.record }
    /// Consumes the plan and returns its unprivileged successor snapshot.
    #[must_use]
    pub fn into_next(self) -> LeaseAggregate { self.next }

    pub(crate) fn into_cas_parts(
        self,
    ) -> (parts: (LeaseAggregate, LeaseTransitionRecord))
        ensures
            parts.0 == self.next,
            parts.1 == self.record,
    {
        (self.next, self.record)
    }
}

/// Move-only logical intersection of a current lease and freshly consumed exact capability.
///
/// The value is bound to one action and the earlier of both exclusive expiry boundaries. It is not
/// a durable receipt, committed holder handle, mutation bundle, or target-gateway permit.
///
/// ```compile_fail
/// use peritus_leases::LeaseUseTransition;
/// fn require_clone<T: Clone>() {}
/// require_clone::<LeaseUseTransition>();
/// ```
pub struct LeaseUseTransition {
    pub(crate) lease: LeaseTransition,
    pub(crate) capability_use: CapabilityUseTransition,
    pub(crate) claim: LeaseClaim,
    pub(crate) effective_expires_at: AuthorityInstant,
}

impl LeaseUseTransition {
    /// Returns the exact effective expiry used by specifications.
    pub closed spec fn spec_effective_expires_at(&self) -> AuthorityInstant {
        self.effective_expires_at
    }
    /// Borrows the lease state plan.
    #[must_use]
    pub const fn lease_transition(&self) -> &LeaseTransition { &self.lease }
    /// Borrows the exact policy logical-use transition.
    #[must_use]
    pub const fn capability_use(&self) -> &CapabilityUseTransition { &self.capability_use }
    /// Returns the exact current claim intersected with policy.
    #[must_use]
    pub const fn claim(&self) -> LeaseClaim { self.claim }
    /// Returns the exact action identity.
    #[must_use]
    pub const fn action_id(&self) -> ActionId { self.capability_use.action_id() }
    /// Returns the exact action digest.
    #[must_use]
    pub const fn action_digest(&self) -> Sha256Digest { self.capability_use.action_digest() }
    /// Returns the earlier exclusive capability/lease expiry.
    #[must_use]
    pub const fn effective_expires_at(&self) -> (expires_at: AuthorityInstant)
        ensures expires_at == self.spec_effective_expires_at(),
    {
        self.effective_expires_at
    }
    /// Consumes the logical intersection into its two still-unprivileged components.
    #[must_use]
    pub fn into_parts(self) -> (LeaseTransition, CapabilityUseTransition) {
        (self.lease, self.capability_use)
    }
}

fn minted_transition(
    command: crate::MintLease,
    next: LeaseAggregate,
) -> (result: LeaseTransition)
    requires
        next.scope == command.scope,
        next.generation.spec_value() == 1,
        next.version.spec_value() == 1,
        next.state == LeaseState::Available,
        next.authority_time.spec_epoch() == command.observed_at.spec_epoch(),
        next.authority_time.spec_greatest_tick_millis()
            == command.observed_at.spec_tick_millis(),
    ensures
        crate::model::concrete_mint_edge(&result.next, result.record, command),
        crate::model::concrete_mint_record(&result.next, result.record),
{
    let record = LeaseTransitionRecord {
        command_id: command.command_id,
        scope: next.scope,
        before_version: None,
        after_version: next.version,
        before_generation: None,
        after_generation: next.generation,
        before_phase: None,
        after_phase: LeasePhase::Available,
        kind: LeaseTransitionKind::Minted,
        binding: Box::new(crate::LeaseCommandBinding::mint(command)),
    };
    LeaseTransition { next, record }
}

} // verus!
