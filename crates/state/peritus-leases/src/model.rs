//! Abstract reachability model and mathematical lease predicates.

pub mod concrete;
#[cfg(verus_only)]
pub use concrete::*;

use vstd::prelude::*;

verus! {

/// Fully defined abstract lease state with no trusted/generated proof helpers.
#[cfg(verus_only)]
pub struct LeaseReachabilityState {
    /// `0=available, 1=active, 2=reconciling, 3=quarantined, 4=retired`.
    pub phase: int,
    /// Current generation.
    pub generation: int,
    /// Aggregate version.
    pub version: int,
    /// Whether an exact active holder is present.
    pub has_holder: bool,
    /// Abstract holder marker; exact nominal holder equality is proved by the concrete model.
    pub holder: int,
}

/// Complete abstract transition family implemented by the executable reducers.
#[cfg(verus_only)]
pub enum LeaseReachabilityStep {
    /// Available to active.
    Acquire,
    /// Active to active renewal or use.
    RenewOrUse,
    /// Active fence with direct availability.
    FenceAvailable,
    /// Active fence pending reconciliation.
    FenceReconciling,
    /// Safe reconciliation.
    ReconcileSafe,
    /// Dirty or indeterminate reconciliation.
    ReconcileUnsafe,
    /// Fail-closed terminal transition.
    Retire,
}

/// Initial abstract state established by mint.
pub open spec fn lease_reachability_init(state: LeaseReachabilityState) -> bool {
    state.phase == 0
        && state.generation == 1
        && state.version == 1
        && !state.has_holder
        && state.holder == 0
}

/// Total, fully defined abstract step relation.
pub open spec fn lease_reachability_step_by(
    pre: LeaseReachabilityState,
    post: LeaseReachabilityState,
    step: LeaseReachabilityStep,
) -> bool {
    match step {
        LeaseReachabilityStep::Acquire => {
            pre.phase == 0
                && pre.version < (u64::MAX - 1) as int
                && post.phase == 1
                && post.generation == pre.generation
                && post.version == pre.version + 1
                && post.has_holder
                && post.holder == 1
        }
        LeaseReachabilityStep::RenewOrUse => {
            pre.phase == 1
                && pre.version < (u64::MAX - 1) as int
                && post.phase == 1
                && post.generation == pre.generation
                && post.version == pre.version + 1
                && post.has_holder
                && post.holder == pre.holder
        }
        LeaseReachabilityStep::FenceAvailable => {
            pre.phase == 1
                && post.phase == 0
                && post.generation == pre.generation + 1
                && post.version == pre.version + 1
                && !post.has_holder
                && post.holder == 0
        }
        LeaseReachabilityStep::FenceReconciling => {
            pre.phase == 1
                && post.phase == 2
                && post.generation == pre.generation + 1
                && post.version == pre.version + 1
                && !post.has_holder
                && post.holder == 0
        }
        LeaseReachabilityStep::ReconcileSafe => {
            pre.phase == 2
                && post.phase == 0
                && post.generation == pre.generation
                && post.version == pre.version + 1
                && !post.has_holder
                && post.holder == 0
        }
        LeaseReachabilityStep::ReconcileUnsafe => {
            pre.phase == 2
                && post.phase == 3
                && post.generation == pre.generation
                && post.version == pre.version + 1
                && !post.has_holder
                && post.holder == 0
        }
        LeaseReachabilityStep::Retire => {
            (pre.phase == 1 || pre.phase == 2)
                && post.phase == 4
                && post.generation == pre.generation
                && post.version == pre.version + 1
                && !post.has_holder
                && post.holder == 0
        }
    }
}

/// Total abstraction from the executable snapshot into the reachability state.
pub(crate) open spec fn abstract_reachability_state(
    aggregate: &crate::LeaseAggregate,
) -> LeaseReachabilityState {
    let (phase, has_holder, holder) = match aggregate.state {
        crate::state::LeaseState::Available => (0int, false, 0int),
        crate::state::LeaseState::Active(_) => (1int, true, 1int),
        crate::state::LeaseState::Reconciling(_) => (2int, false, 0int),
        crate::state::LeaseState::Quarantined(_) => (3int, false, 0int),
        crate::state::LeaseState::Retired(_) => (4int, false, 0int),
    };
    LeaseReachabilityState {
        phase,
        generation: aggregate.generation.spec_value(),
        version: aggregate.version.spec_value(),
        has_holder,
        holder,
    }
}

/// The fully defined abstract machine accepts the executable before/after abstraction.
pub(crate) open spec fn lease_reachability_step(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
) -> bool {
    let pre = abstract_reachability_state(before);
    let post = abstract_reachability_state(after);
    lease_reachability_step_by(pre, post, LeaseReachabilityStep::Acquire)
        || lease_reachability_step_by(pre, post, LeaseReachabilityStep::RenewOrUse)
        || lease_reachability_step_by(pre, post, LeaseReachabilityStep::FenceAvailable)
        || lease_reachability_step_by(pre, post, LeaseReachabilityStep::FenceReconciling)
        || lease_reachability_step_by(pre, post, LeaseReachabilityStep::ReconcileSafe)
        || lease_reachability_step_by(pre, post, LeaseReachabilityStep::ReconcileUnsafe)
        || lease_reachability_step_by(pre, post, LeaseReachabilityStep::Retire)
}

/// Whether one abstract claim is current for an exact aggregate generation and holder.
pub open spec fn logical_claim_is_current(
    current_generation: int,
    current_holder: int,
    claim_generation: int,
    claim_holder: int,
) -> bool {
    current_generation == claim_generation && current_holder == claim_holder
}

/// Mathematical fencing relation.
pub open spec fn generation_is_fenced(before: int, after: int) -> bool {
    after > before
}

/// Mathematical aggregate-time-floor acceptance relation.
pub open spec fn time_floor_accepts(
    floor_epoch: int,
    floor_tick: int,
    candidate_epoch: int,
    candidate_tick: int,
) -> bool {
    floor_epoch == candidate_epoch && floor_tick <= candidate_tick
}

/// Exact logical authority intersection for a single action.
pub open spec fn exact_authority_intersection(
    lease_actor: int,
    policy_actor: int,
    lease_environment: int,
    policy_environment: int,
    lease_workspace: int,
    policy_workspace: int,
    lease_generation: int,
    policy_generation: int,
    lease_resource: int,
    policy_resource: int,
) -> bool {
    lease_actor == policy_actor
        && lease_environment == policy_environment
        && lease_workspace == policy_workspace
        && lease_generation == policy_generation
        && lease_resource == policy_resource
}

/// Mathematical successor-version relation for an accepted ordinary edge.
pub open spec fn version_advances_once(before: int, after: int) -> bool {
    after == before + 1
}

} // verus!
