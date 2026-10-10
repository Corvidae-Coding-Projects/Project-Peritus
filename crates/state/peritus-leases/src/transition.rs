//! Pure checked reducers and move-only logical transition values.
#![allow(
    missing_docs,
    reason = "pinned Cargo-Verus synthesizes undocumented accessors for documented payload variants"
)]

mod authority;
mod authority_validation;

mod core;

mod fencing;
mod fencing_apply;
mod fencing_rejections;

mod fencing_retire;
mod fencing_validation;
mod lifecycle;
mod reconciliation;
mod record_duplication;
mod validation;

use self::validation::{
    earlier, ensure_before_expiry, map_policy_time, next_active_version, next_non_fence_version,
    require_active, require_active_claim, require_phase, validate_observation,
};

use self::core::{AuthorityTimeAdvance, TransitionPlan, rejection, transition};
use crate::state::LeaseState;
use crate::{
    LeaseAggregate, LeaseClaim, LeaseError, LeasePhase, LeaseTransitionFailure, RetirementReason,
};
use peritus_policy::{AuthorityInstant, CapabilityUseTransition};
use peritus_types::{ActionId, CommandId, Generation, RevisionNumber, Sha256Digest};

#[doc = " Semantic kind carried by every compare-and-swap plan."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LeaseTransitionKind {
    #[doc = " Established a previously absent aggregate."]
    Minted,
    #[doc = " Acquired an available generation."]
    Acquired,
    #[doc = " Strictly extended an active claim."]
    Renewed,
    #[doc = " Consumed a logical lease/policy intersection for one action."]
    Used {
        #[doc = " Exact action identity."]
        action_id: ActionId,
        #[doc = " Exact action digest bound by policy."]
        action_digest: Sha256Digest,
    },
    #[doc = " Released and fenced with exact quiescence evidence."]
    ReleasedAvailable,
    #[doc = " Released and fenced pending reconciliation."]
    ReleasedReconciling,
    #[doc = " Expired and fenced pending reconciliation."]
    Expired,
    #[doc = " Fenced after exact holder-loss evidence."]
    HolderLost,
    #[doc = " Fenced across an explicit clock discontinuity."]
    ClockDiscontinuity,
    #[doc = " Fenced after a separately authorized revocation."]
    Revoked,
    #[doc = " Accepted safe correlated evidence and became available."]
    ReconciledAvailable,
    #[doc = " Accepted dirty or indeterminate evidence and quarantined."]
    ReconciledQuarantined,
    #[doc = " Fencing retired the aggregate."]
    Retired(RetirementReason),
}
#[doc = " Unprivileged exact typed record for one accepted logical state edge."]
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
    #[doc = " Returns the idempotency identity."]
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    #[doc = " Returns the exact aggregate scope."]
    #[must_use]
    pub const fn scope(&self) -> crate::LeaseScope {
        self.scope
    }
    #[doc = " Returns the expected prior version, or absence for mint."]
    #[must_use]
    pub const fn before_version(&self) -> Option<RevisionNumber> {
        self.before_version
    }
    #[doc = " Returns the planned successor version."]
    #[must_use]
    pub const fn after_version(&self) -> RevisionNumber {
        self.after_version
    }
    #[doc = " Returns the prior generation, or absence for mint."]
    #[must_use]
    pub const fn before_generation(&self) -> Option<Generation> {
        self.before_generation
    }
    #[doc = " Returns the successor generation."]
    #[must_use]
    pub const fn after_generation(&self) -> Generation {
        self.after_generation
    }
    #[doc = " Returns the prior phase, or absence for mint."]
    #[must_use]
    pub const fn before_phase(&self) -> Option<LeasePhase> {
        self.before_phase
    }
    #[doc = " Returns the successor phase."]
    #[must_use]
    pub const fn after_phase(&self) -> LeasePhase {
        self.after_phase
    }
    #[doc = " Returns the semantic edge kind."]
    #[must_use]
    pub const fn kind(&self) -> LeaseTransitionKind {
        self.kind
    }
    #[doc = " Returns the exact unprivileged source-command projection."]
    #[must_use]
    pub fn binding(&self) -> &crate::LeaseCommandBinding {
        &self.binding
    }
}
#[doc = " Move-only pure transition plan. It does not prove persistence or authorize an effect."]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " use peritus_leases::LeaseTransition;"]
#[doc = " fn require_clone<T: Clone>() {}"]
#[doc = " require_clone::<LeaseTransition>();"]
#[doc = " ```"]
pub struct LeaseTransition {
    pub(crate) next: LeaseAggregate,
    pub(crate) record: LeaseTransitionRecord,
}
#[doc = " Move-only result of a reducer that must preserve its linear input on rejection."]
#[doc = ""]
#[doc = " Both variants own their complete result. The rejected variant carries the exact unchanged"]
#[doc = " aggregate and its authority-time floor through [`LeaseTransitionFailure`]."]
#[must_use = "the accepted transition or preserving rejection must be consumed"]
pub enum LeaseTransitionOutcome {
    #[doc = " The command produced one accepted logical transition."]
    Accepted(LeaseTransition),
    #[doc = " The command was rejected and preserved its input aggregate."]
    Rejected(LeaseTransitionFailure),
}
#[doc = " Move-only result of exact policy-use intersection with a linear lease aggregate."]
#[must_use = "the accepted logical use or preserving rejection must be consumed"]
pub enum LeaseUseOutcome {
    #[doc = " The command produced one accepted logical lease/policy intersection."]
    Accepted(LeaseUseTransition),
    #[doc = " The command was rejected and preserved its input aggregate."]
    Rejected(crate::LeaseUseFailure),
}
impl LeaseTransition {
    #[doc = " Borrows the planned successor snapshot."]
    #[must_use]
    pub const fn next(&self) -> &LeaseAggregate {
        &self.next
    }
    #[doc = " Returns the unprivileged typed state-edge record."]
    #[must_use]
    pub const fn record(&self) -> &LeaseTransitionRecord {
        &self.record
    }
    #[doc = " Consumes the plan and returns its unprivileged successor snapshot."]
    #[must_use]
    pub fn into_next(self) -> LeaseAggregate {
        self.next
    }
    pub(crate) fn into_cas_parts(self) -> (LeaseAggregate, LeaseTransitionRecord) {
        (self.next, self.record)
    }
}
#[doc = " Move-only logical intersection of a current lease and freshly consumed exact capability."]
#[doc = ""]
#[doc = " The value is bound to one action and the earlier of both exclusive expiry boundaries. It is not"]
#[doc = " a durable receipt, committed holder handle, mutation bundle, or target-gateway permit."]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " use peritus_leases::LeaseUseTransition;"]
#[doc = " fn require_clone<T: Clone>() {}"]
#[doc = " require_clone::<LeaseUseTransition>();"]
#[doc = " ```"]
pub struct LeaseUseTransition {
    pub(crate) lease: LeaseTransition,
    pub(crate) capability_use: CapabilityUseTransition,
    pub(crate) claim: LeaseClaim,
    pub(crate) effective_expires_at: AuthorityInstant,
}
impl LeaseUseTransition {
    #[doc = " Borrows the lease state plan."]
    #[must_use]
    pub const fn lease_transition(&self) -> &LeaseTransition {
        &self.lease
    }
    #[doc = " Borrows the exact policy logical-use transition."]
    #[must_use]
    pub const fn capability_use(&self) -> &CapabilityUseTransition {
        &self.capability_use
    }
    #[doc = " Returns the exact current claim intersected with policy."]
    #[must_use]
    pub const fn claim(&self) -> LeaseClaim {
        self.claim
    }
    #[doc = " Returns the exact action identity."]
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.capability_use.action_id()
    }
    #[doc = " Returns the exact action digest."]
    #[must_use]
    pub const fn action_digest(&self) -> Sha256Digest {
        self.capability_use.action_digest()
    }
    #[doc = " Returns the earlier exclusive capability/lease expiry."]
    #[must_use]
    pub const fn effective_expires_at(&self) -> AuthorityInstant {
        self.effective_expires_at
    }
    #[doc = " Consumes the logical intersection into its two still-unprivileged components."]
    #[must_use]
    pub fn into_parts(self) -> (LeaseTransition, CapabilityUseTransition) {
        (self.lease, self.capability_use)
    }
}
fn minted_transition(command: crate::MintLease, next: LeaseAggregate) -> LeaseTransition {
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
