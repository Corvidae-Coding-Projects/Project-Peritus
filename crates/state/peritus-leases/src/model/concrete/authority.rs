//! Executable lease/policy intersection refinement predicates.

use vstd::prelude::*;

verus! {

/// Exact lease/policy scope and time intersection accepted by executable validation.
pub(crate) open spec fn concrete_policy_intersection(
    aggregate: &crate::LeaseAggregate,
    command: &crate::UseLease,
) -> bool {
    super::concrete_instant_matches(command.capability_use.spec_used_at(), command.observed_at)
        && super::concrete_identifier_matches(
            command.capability_use.spec_scope_actor_id(),
            command.claim.holder.actor_id.spec_bytes(),
        )
        && super::concrete_identifier_matches(
            command.capability_use.spec_scope_environment_id(),
            aggregate.scope.environment.spec_bytes(),
        )
        && super::concrete_identifier_matches(
            command
                .capability_use
                .spec_scope_revision()
                .spec_workspace_id()
                .spec_bytes(),
            aggregate.scope.workspace.spec_bytes(),
        )
        && command
            .capability_use
            .spec_scope_revision()
            .spec_workspace_generation()
            .spec_value()
            == aggregate.generation.spec_value()
        && super::concrete_identifier_matches(
            command.capability_use.spec_permission_resource_id(),
            aggregate.scope.resource.spec_bytes(),
        )
        && command.capability_use.spec_scope_validity().spec_contains(command.observed_at)
}

/// The executable lease-use output is the exact current lease/policy intersection.
pub(crate) open spec fn concrete_lease_use_is_current(
    use_transition: &crate::LeaseUseTransition,
) -> bool {
    super::concrete_claim_is_current(&use_transition.lease.next, use_transition.claim)
        && super::concrete_identifier_matches(
            use_transition.capability_use.spec_scope_actor_id(),
            use_transition.claim.holder.actor_id.spec_bytes(),
        )
        && super::concrete_identifier_matches(
            use_transition.capability_use.spec_scope_environment_id(),
            use_transition.claim.scope.environment.spec_bytes(),
        )
        && super::concrete_identifier_matches(
            use_transition.capability_use.spec_permission_resource_id(),
            use_transition.claim.scope.resource.spec_bytes(),
        )
        && super::concrete_identifier_matches(
            use_transition
                .capability_use
                .spec_scope_revision()
                .spec_workspace_id()
                .spec_bytes(),
            use_transition.claim.scope.workspace.spec_bytes(),
        )
        && use_transition
            .capability_use
            .spec_scope_revision()
            .spec_workspace_generation()
            .spec_value()
            == use_transition.claim.generation.spec_value()
        && use_transition.capability_use.spec_used_at().spec_epoch()
            == use_transition.claim.expires_at.spec_epoch()
        && use_transition.capability_use.spec_used_at().spec_tick_millis()
            < use_transition.claim.expires_at.spec_tick_millis()
        && super::concrete_instant_matches(
            use_transition.spec_effective_expires_at(),
            if use_transition.claim.expires_at.spec_tick_millis()
                <= use_transition
                    .capability_use
                    .spec_scope_validity()
                    .spec_expires_at()
                    .spec_tick_millis()
            {
                use_transition.claim.expires_at
            } else {
                use_transition
                    .capability_use
                    .spec_scope_validity()
                    .spec_expires_at()
            },
        )
        && match use_transition.lease.record.kind {
            crate::LeaseTransitionKind::Used { action_id, action_digest } => {
                action_id.spec_bytes() == use_transition.capability_use.spec_action_id()
                    && action_digest.spec_bytes()
                        == use_transition.capability_use.spec_action_digest()
            }
            _ => false,
        }
}

/// Exact accepted logical-use decision and its abstract-machine refinement.
pub(crate) open spec fn concrete_use_edge(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseUseTransition,
    command_id: peritus_types::CommandId,
) -> bool {
    super::concrete_record_matches(before, &transition.lease.next, transition.lease.record)
        && super::concrete_refines_reachability_step(before, &transition.lease.next)
        && super::concrete_claim_is_current(before, transition.claim)
        && transition.lease.record.command_id == command_id
        && transition.lease.record.binding.matches_use_transition(transition)
        && match (before.state, transition.lease.next.state) {
            (
                crate::state::LeaseState::Active(previous),
                crate::state::LeaseState::Active(next),
            ) => previous == next,
            _ => false,
        }
        && super::concrete_time_observed(
            before,
            &transition.lease.next,
            transition.capability_use.spec_used_at(),
        )
        && concrete_lease_use_is_current(transition)
}

/// Public-contract wrapper for an accepted move-only lease/policy logical use.
pub closed spec fn concrete_use_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseUseTransition,
    command: &crate::UseLease,
) -> bool {
    concrete_use_edge(before, transition, command.spec_command_id())
        && transition.lease.record.binding.matches_use(command)
}

/// Exact unchanged-aggregate contract for rejected lease/policy intersections.
pub closed spec fn concrete_use_rejection_preserves_input(
    before: &crate::LeaseAggregate,
    failure: &crate::LeaseUseFailure,
    command: &crate::UseLease,
) -> bool {
    super::preservation::concrete_snapshot_preserved(before, &failure.spec_aggregate())
        && failure.spec_command() == *command
}

/// Exact rejected logical-use ownership contract, including error and full source command.
pub closed spec fn concrete_use_rejection(
    before: &crate::LeaseAggregate,
    failure: &crate::LeaseUseFailure,
    command: &crate::UseLease,
    error: crate::LeaseError,
) -> bool {
    concrete_use_rejection_preserves_input(before, failure, command)
        && failure.spec_error() == error
        && failure.spec_command() == *command
}

/// Establishes policy-use rejection preservation from its owned lease failure.
pub(crate) proof fn establish_use_rejection_preservation(
    before: &crate::LeaseAggregate,
    lease: &crate::LeaseTransitionFailure,
    failure: &crate::LeaseUseFailure,
    command: &crate::UseLease,
)
    requires
        super::preservation::concrete_rejection_preserves_input(before, lease),
        failure.spec_aggregate() == lease.spec_aggregate(),
        failure.spec_command() == *command,
    ensures concrete_use_rejection_preserves_input(before, failure, command),
{
    super::preservation::rejection_implies_snapshot_preserved(before, lease);
}

/// Establishes exact rejected-command ownership from the preserving lease failure.
pub(crate) proof fn establish_use_rejection(
    before: &crate::LeaseAggregate,
    lease: &crate::LeaseTransitionFailure,
    failure: &crate::LeaseUseFailure,
    command: &crate::UseLease,
    error: crate::LeaseError,
)
    requires
        super::preservation::concrete_rejection_preserves_input(before, lease),
        lease.spec_error() == error,
        failure.spec_aggregate() == lease.spec_aggregate(),
        failure.spec_error() == lease.spec_error(),
        failure.spec_command() == *command,
    ensures
        concrete_use_rejection(before, failure, command, error),
        concrete_use_rejection_preserves_input(before, failure, command),
{
    establish_use_rejection_preservation(before, lease, failure, command);
}

/// Establishes the opaque public logical-use contract from its exact internal edge.
pub(crate) proof fn establish_use_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseUseTransition,
    command: &crate::UseLease,
)
    requires
        concrete_use_edge(before, transition, command.spec_command_id()),
        transition.lease.record.binding.matches_use(command),
    ensures concrete_use_transition(before, transition, command),
{
}

/// Projects current-claim authority from an exact executable lease-use intersection.
pub(crate) proof fn lease_use_implies_current_claim(
    logical_use: &crate::LeaseUseTransition,
)
    requires concrete_lease_use_is_current(logical_use),
    ensures super::concrete_claim_is_current(&logical_use.lease.next, logical_use.claim),
{
}

} // verus!
