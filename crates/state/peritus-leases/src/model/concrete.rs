//! Refinement predicates over the executable lease representation.

pub mod authority;
pub mod fencing;
pub mod fence_commands;
pub mod identity;
pub mod preservation;
pub mod rejections;
#[cfg(verus_only)]
pub use authority::*;
#[cfg(verus_only)]
pub use fencing::*;
#[cfg(verus_only)]
pub use fence_commands::*;
#[cfg(verus_only)]
pub use identity::*;
#[cfg(verus_only)]
pub use preservation::*;

use vstd::prelude::*;

verus! {

/// Exact current-claim relation over the executable private representation.
pub(crate) open spec fn concrete_claim_is_current(
    aggregate: &crate::LeaseAggregate,
    claim: crate::LeaseClaim,
) -> bool {
    match aggregate.state {
        crate::state::LeaseState::Active(active) => {
            concrete_scope_matches(claim.scope, aggregate.scope)
                && concrete_holder_matches(claim.holder, active.holder)
                && claim.generation.spec_value() == aggregate.generation.spec_value()
                && claim.claim_version.spec_value() == active.claim_version.spec_value()
                && concrete_instant_matches(claim.issued_at, active.issued_at)
                && concrete_instant_matches(claim.expires_at, active.expires_at)
        }
        _ => false,
    }
}

/// The executable aggregate representation contains at most one exact active holder.
pub(crate) open spec fn concrete_exclusive(aggregate: &crate::LeaseAggregate) -> bool {
    match aggregate.state {
        crate::state::LeaseState::Active(active) => {
            forall |candidate: crate::LeaseClaim| #![auto]
                concrete_claim_is_current(aggregate, candidate)
                    ==> concrete_holder_matches(candidate.holder, active.holder)
        }
        _ => forall |candidate: crate::LeaseClaim| #![auto]
            !concrete_claim_is_current(aggregate, candidate),
    }
}

/// Exact refinement relation between executable before/after snapshots and their typed record.
pub(crate) open spec fn concrete_record_matches(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    record: crate::LeaseTransitionRecord,
) -> bool {
    record.scope == before.scope
        && record.scope == after.scope
        && record.before_version == Some(before.version)
        && record.after_version == after.version
        && after.version.spec_value() == before.version.spec_value() + 1
        && record.before_generation == Some(before.generation)
        && record.after_generation == after.generation
        && record.before_phase == Some(before.internal_phase())
        && record.after_phase == after.internal_phase()
}

/// Exact successor values selected by the private transition constructor.
pub(crate) open spec fn concrete_transition_matches(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    record: crate::LeaseTransitionRecord,
    command_id: peritus_types::CommandId,
    generation: peritus_types::Generation,
    state: crate::state::LeaseState,
    kind: crate::LeaseTransitionKind,
    binding: crate::LeaseCommandBinding,
) -> bool {
    concrete_record_matches(before, after, record)
        && record.command_id == command_id
        && after.generation == generation
        && after.state == state
        && record.kind == kind
        && *record.binding == binding
}

/// Exact refinement relation for mint, whose durable expectation is absence.
pub(crate) open spec fn concrete_mint_edge(
    after: &crate::LeaseAggregate,
    record: crate::LeaseTransitionRecord,
    command: crate::MintLease,
) -> bool {
    record.command_id == command.command_id
        && record.binding.matches_mint(command)
        && record.scope == command.scope
        && record.scope == after.scope
        && record.before_version.is_none()
        && record.after_version == after.version
        && after.version.spec_value() == 1
        && record.before_generation.is_none()
        && record.after_generation == after.generation
        && after.generation.spec_value() == 1
        && record.before_phase.is_none()
        && record.after_phase == crate::LeasePhase::Available
        && after.internal_phase() == crate::LeasePhase::Available
        && matches!(record.kind, crate::LeaseTransitionKind::Minted)
        && after.authority_time.spec_epoch() == command.observed_at.spec_epoch()
        && after.authority_time.spec_greatest_tick_millis()
            == command.observed_at.spec_tick_millis()
}

/// Public-contract wrapper for an accepted move-only mint transition.
pub closed spec fn concrete_mint_transition(
    transition: &crate::LeaseTransition,
    command: crate::MintLease,
) -> bool {
    concrete_mint_edge(&transition.next, transition.record, command)
        && concrete_mint_record(&transition.next, transition.record)
}

/// Backward-compatible mint-record projection used by record-only lemmas.
pub(crate) open spec fn concrete_mint_record(
    after: &crate::LeaseAggregate,
    record: crate::LeaseTransitionRecord,
) -> bool {
    record.scope == after.scope
        && record.before_version.is_none()
        && record.after_version == after.version
        && record.before_generation.is_none()
        && record.after_generation == after.generation
        && record.before_phase.is_none()
        && record.after_phase == after.internal_phase()
        && matches!(record.kind, crate::LeaseTransitionKind::Minted)
}

/// A successful same-epoch observation advances the aggregate time floor exactly to the candidate.
pub(crate) open spec fn concrete_time_observed(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    observed_at: peritus_policy::AuthorityInstant,
) -> bool {
    after.authority_time.spec_epoch() == before.authority_time.spec_epoch()
        && after.authority_time.spec_epoch() == observed_at.spec_epoch()
        && after.authority_time.spec_greatest_tick_millis()
            == observed_at.spec_tick_millis()
        && after.authority_time.spec_greatest_tick_millis()
            >= before.authority_time.spec_greatest_tick_millis()
}

/// Exact fail-closed time-floor replacement used only by clock-discontinuity fencing.
pub(crate) open spec fn concrete_discontinuity_time(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    observed_at: peritus_policy::AuthorityInstant,
) -> bool {
    if observed_at.spec_epoch() == before.authority_time.spec_epoch() {
        after.authority_time == before.authority_time
            && observed_at.spec_tick_millis()
                < before.authority_time.spec_greatest_tick_millis()
    } else {
        after.authority_time.spec_epoch() == observed_at.spec_epoch()
            && after.authority_time.spec_greatest_tick_millis()
                == observed_at.spec_tick_millis()
    }
}

/// Projection of one executable transition onto the ordinary reachability machine fields.
pub(crate) open spec fn concrete_refines_reachability_step(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
) -> bool {
    after.version.spec_value() == before.version.spec_value() + 1
        && match before.state {
            crate::state::LeaseState::Available => {
                matches!(after.state, crate::state::LeaseState::Active(_))
                    && after.generation == before.generation
            }
            crate::state::LeaseState::Active(_) => match after.state {
                crate::state::LeaseState::Active(_) => after.generation == before.generation,
                crate::state::LeaseState::Available
                | crate::state::LeaseState::Reconciling(_) => {
                    after.generation.spec_value() == before.generation.spec_value() + 1
                }
                crate::state::LeaseState::Retired(_) => after.generation == before.generation,
                _ => false,
            },
            crate::state::LeaseState::Reconciling(_) => {
                matches!(
                    after.state,
                    crate::state::LeaseState::Available
                        | crate::state::LeaseState::Quarantined(_)
                        | crate::state::LeaseState::Retired(_)
                ) && after.generation == before.generation
            }
            _ => false,
        }
        && crate::model::lease_reachability_step(before, after)
}

/// Exact accepted acquisition decision and its abstract-machine refinement.
pub(crate) open spec fn concrete_acquire_edge(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    record: crate::LeaseTransitionRecord,
    command: crate::AcquireLease,
) -> bool {
    concrete_record_matches(before, after, record)
        && concrete_refines_reachability_step(before, after)
        && record.command_id == command.command_id
        && record.binding.matches_acquire(command)
        && matches!(record.kind, crate::LeaseTransitionKind::Acquired)
        && matches!(before.state, crate::state::LeaseState::Available)
        && match after.state {
            crate::state::LeaseState::Active(active) => {
                active.holder == command.holder
                    && active.claim_version.spec_value() == 1
                    && active.issued_at == command.observed_at
                    && active.expires_at.spec_epoch()
                        == command.observed_at.spec_epoch()
                    && active.expires_at.spec_tick_millis()
                        == command.observed_at.spec_tick_millis()
                            + command.duration.spec_millis()
            }
            _ => false,
        }
        && concrete_time_observed(before, after, command.observed_at)
}

/// Public-contract wrapper for an accepted move-only acquisition transition.
pub closed spec fn concrete_acquire_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::AcquireLease,
) -> bool {
    concrete_acquire_edge(before, &transition.next, transition.record, command)
}

/// Exact accepted renewal decision and its abstract-machine refinement.
pub(crate) open spec fn concrete_renew_edge(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    record: crate::LeaseTransitionRecord,
    command: crate::RenewLease,
) -> bool {
    concrete_record_matches(before, after, record)
        && concrete_refines_reachability_step(before, after)
        && concrete_claim_is_current(before, command.claim)
        && command.observed_at.spec_epoch() == command.claim.expires_at.spec_epoch()
        && command.observed_at.spec_tick_millis() < command.claim.expires_at.spec_tick_millis()
        && record.command_id == command.command_id
        && record.binding.matches_renew(command)
        && matches!(record.kind, crate::LeaseTransitionKind::Renewed)
        && match (before.state, after.state) {
            (
                crate::state::LeaseState::Active(previous),
                crate::state::LeaseState::Active(next),
            ) => {
                next.holder == previous.holder
                    && next.claim_version.spec_value()
                        == previous.claim_version.spec_value() + 1
                    && next.issued_at == command.observed_at
                    && next.expires_at.spec_epoch()
                        == command.observed_at.spec_epoch()
                    && next.expires_at.spec_tick_millis()
                        == command.observed_at.spec_tick_millis()
                            + command.duration.spec_millis()
                    && next.expires_at.spec_tick_millis()
                        > previous.expires_at.spec_tick_millis()
            }
            _ => false,
        }
        && concrete_time_observed(before, after, command.observed_at)
}

/// Public-contract wrapper for an accepted move-only renewal transition.
pub closed spec fn concrete_renew_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::RenewLease,
) -> bool {
    concrete_renew_edge(before, &transition.next, transition.record, command)
}

/// Establishes the opaque public mint contract from the exact internal edge.
pub(crate) proof fn establish_mint_transition(
    transition: &crate::LeaseTransition,
    command: crate::MintLease,
)
    requires
        concrete_mint_edge(&transition.next, transition.record, command),
        concrete_mint_record(&transition.next, transition.record),
    ensures concrete_mint_transition(transition, command),
{
}

/// Establishes the opaque public acquisition contract from the exact internal edge.
pub(crate) proof fn establish_acquire_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::AcquireLease,
)
    requires concrete_acquire_edge(before, &transition.next, transition.record, command),
    ensures concrete_acquire_transition(before, transition, command),
{
}

/// Establishes the opaque public renewal contract from the exact internal edge.
pub(crate) proof fn establish_renew_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::RenewLease,
)
    requires concrete_renew_edge(before, &transition.next, transition.record, command),
    ensures concrete_renew_transition(before, transition, command),
{
}

/// Establishes the concrete exclusive-holder predicate from the private representation.
pub(crate) proof fn establish_concrete_exclusive(aggregate: &crate::LeaseAggregate)
    ensures concrete_exclusive(aggregate),
{
    match aggregate.state {
        crate::state::LeaseState::Active(active) => {
            assert forall |candidate: crate::LeaseClaim| #![auto]
                concrete_claim_is_current(aggregate, candidate)
                    implies concrete_holder_matches(candidate.holder, active.holder) by {
            }
        }
        _ => {
            assert forall |candidate: crate::LeaseClaim| #![auto]
                !concrete_claim_is_current(aggregate, candidate) by {
            }
        }
    }
}

/// Projects identical holder, generation, and claim version from two current claims.
pub(crate) proof fn current_claims_match(
    aggregate: &crate::LeaseAggregate,
    first: crate::LeaseClaim,
    second: crate::LeaseClaim,
)
    requires
        concrete_claim_is_current(aggregate, first),
        concrete_claim_is_current(aggregate, second),
    ensures
        concrete_holder_matches(first.holder, second.holder),
        first.generation.spec_value() == second.generation.spec_value(),
        first.claim_version.spec_value() == second.claim_version.spec_value(),
{
    match aggregate.state {
        crate::state::LeaseState::Active(active) => {
            current_holders_match(first.holder, active.holder, second.holder);
        }
        _ => assert(false),
    }
}

/// Projects every exact field recorded by the private transition constructor.
pub(crate) proof fn project_concrete_record(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    record: crate::LeaseTransitionRecord,
)
    requires concrete_record_matches(before, after, record),
    ensures
        record.scope == before.scope,
        record.scope == after.scope,
        record.before_generation == Some(before.generation),
        record.after_generation == after.generation,
        record.before_version == Some(before.version),
        record.after_version == after.version,
{
}

} // verus!
