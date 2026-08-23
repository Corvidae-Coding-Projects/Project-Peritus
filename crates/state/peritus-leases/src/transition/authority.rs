//! Exact policy-use and current-lease intersection reducer.

use super::{
    earlier, ensure_before_expiry, next_active_version, rejection,
    require_active_claim, transition, validate_observation, AuthorityTimeAdvance, LeaseAggregate,
    LeaseError, LeaseState, LeaseTransitionKind, LeaseUseTransition, TransitionPlan,
};
use crate::{
    LeaseTransitionOutcome, LeaseUseFailure, LeaseUseOutcome, UseLease,
};
use super::authority_validation::validate_policy_intersection;
use vstd::prelude::*;

verus! {

impl LeaseAggregate {
    /// Intersects a current exact claim with a freshly consumed exact policy capability.
    ///
    /// Failure returns no logical lease use and leaves the caller's aggregate unchanged.
    ///
    /// # Errors
    ///
    /// Returns a typed claim, time, version, policy-use, or exact authority-intersection failure.
    pub fn authorize_use(
        self,
        command: UseLease,
    ) -> (result: LeaseUseOutcome)
        ensures crate::model::concrete::rejections::authority::concrete_use_decision(
            &self,
            result,
            &command,
        ),
    {
        let ghost before = self;
        let ghost attempted = command;
        proof { command.reveal_exact_fields(); }
        let active = match require_active_claim(&self, command.claim()) {
            Ok(value) => value,
            Err(error) => {
                assert(crate::model::concrete::rejections::authority::use_error(
                    &self,
                    &command,
                ) == Some(error));
                return use_rejection(self, error, command);
            }
        };
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            assert(crate::model::concrete::rejections::authority::use_error(
                &self,
                &command,
            ) == Some(error));
            return use_rejection(self, error, command);
        }
        if let Err(error) = ensure_before_expiry(active.expires_at, command.observed_at()) {
            assert(crate::model::concrete::rejections::authority::use_error(
                &self,
                &command,
            ) == Some(error));
            return use_rejection(self, error, command);
        }
        if let Err(error) = validate_policy_intersection(&self, &command) {
            assert(crate::model::concrete::rejections::authority::use_error(
                &self,
                &command,
            ) == Some(error));
            return use_rejection(self, error, command);
        }
        let version = match next_active_version(self.version) {
            Ok(value) => value,
            Err(error) => {
                assert(crate::model::concrete::rejections::authority::use_error(
                    &self,
                    &command,
                ) == Some(error));
                return use_rejection(self, error, command);
            }
        };
        let effective_expires_at = earlier(
            active.expires_at,
            command.capability_use.scope().validity().expires_at(),
        );
        let kind = use_transition_kind(&command);
        let command_id = command.command_id();
        let observed_at = command.observed_at();
        let generation = self.generation;
        let _claim = command.claim();
        let binding = crate::LeaseCommandBinding::use_command(&command);
        let lease = match transition(self, TransitionPlan::new(
            command_id,
            version,
            generation,
            AuthorityTimeAdvance::Observe(observed_at),
            LeaseState::Active(active),
            kind,
            binding,
        )) {
            LeaseTransitionOutcome::Accepted(lease) => lease,
            LeaseTransitionOutcome::Rejected(failure) => {
                let ghost failure_view = failure;
                let use_failure = LeaseUseFailure::new(failure, command);
                proof {
                    establish_late_use_rejection(
                        &before,
                        &failure_view,
                        &use_failure,
                        &attempted,
                    );
                }
                return LeaseUseOutcome::Rejected(use_failure);
            }
        };
        let (_, consumed_claim, _, capability_use) = command.into_parts();
        let accepted = LeaseUseTransition {
            lease,
            capability_use,
            claim: consumed_claim,
            effective_expires_at,
        };
        proof {
            establish_authorized_use(
                &before,
                &accepted,
                &attempted,
                command_id,
                _claim,
                observed_at,
            );
        }
        LeaseUseOutcome::Accepted(accepted)
    }
}

const fn use_transition_kind(command: &UseLease) -> (kind: LeaseTransitionKind)
    ensures match kind {
        LeaseTransitionKind::Used { action_id, action_digest } => {
            action_id.spec_bytes() == command.capability_use.spec_action_id()
                && action_digest.spec_bytes() == command.capability_use.spec_action_digest()
        }
        _ => false,
    },
{
    LeaseTransitionKind::Used {
        action_id: command.capability_use.action_id(),
        action_digest: command.capability_use.action_digest(),
    }
}

proof fn establish_late_use_rejection(
    before: &LeaseAggregate,
    lease: &crate::LeaseTransitionFailure,
    failure: &LeaseUseFailure,
    command: &UseLease,
)
    requires
        lease.spec_error() == LeaseError::CorruptState,
        crate::model::concrete_rejection_preserves_input(before, lease),
        failure.spec_aggregate() == lease.spec_aggregate(),
        failure.spec_error() == lease.spec_error(),
        failure.spec_command() == *command,
        crate::model::concrete::rejections::authority::use_error(before, command)
            == Some(lease.spec_error()),
    ensures crate::model::concrete::rejections::authority::concrete_use_decision(
        before,
        LeaseUseOutcome::Rejected(*failure),
        command,
    ),
{
    crate::model::concrete::authority::establish_use_rejection(
        before,
        lease,
        failure,
        command,
        lease.spec_error(),
    );
    crate::model::concrete::rejections::authority::establish_use_rejection(
        before,
        failure,
        command,
        lease.spec_error(),
    );
}

proof fn establish_authorized_use(
    before: &LeaseAggregate,
    accepted: &LeaseUseTransition,
    command: &UseLease,
    command_id: peritus_types::CommandId,
    claim: crate::LeaseClaim,
    observed_at: peritus_policy::AuthorityInstant,
)
    requires
        command_id == command.command_id,
        command_id == command.spec_command_id(),
        claim == command.claim,
        observed_at == command.observed_at,
        accepted.claim == command.claim,
        accepted.capability_use == command.capability_use,
        crate::model::concrete_record_matches(
            before,
            &accepted.lease.next,
            accepted.lease.record,
        ),
        crate::model::concrete_refines_reachability_step(before, &accepted.lease.next),
        accepted.lease.record.command_id == command_id,
        match (before.state, accepted.lease.next.state) {
            (LeaseState::Active(previous), LeaseState::Active(next)) => previous == next,
            _ => false,
        },
        crate::model::concrete_time_observed(
            before,
            &accepted.lease.next,
            accepted.capability_use.spec_used_at(),
        ),
        match accepted.lease.record.kind {
            LeaseTransitionKind::Used { action_id, action_digest } => {
                action_id.spec_bytes() == accepted.capability_use.spec_action_id()
                    && action_digest.spec_bytes()
                        == accepted.capability_use.spec_action_digest()
            }
            _ => false,
        },
        crate::model::concrete_claim_is_current(before, accepted.claim),
        crate::model::concrete_claim_is_current(&accepted.lease.next, accepted.claim),
        crate::model::concrete_policy_intersection(before, command),
        crate::model::concrete_instant_matches(
            accepted.spec_effective_expires_at(),
            if accepted.claim.expires_at.spec_tick_millis()
                <= accepted
                    .capability_use
                    .spec_scope_validity()
                    .spec_expires_at()
                    .spec_tick_millis()
            {
                accepted.claim.expires_at
            } else {
                accepted.capability_use.spec_scope_validity().spec_expires_at()
            },
        ),
        accepted.lease.record.binding.matches_use(command),
        accepted.lease.record.binding.matches_use_capability(&accepted.capability_use),
        accepted.lease.record.binding.matches_use_lease_inputs(
            command_id,
            accepted.claim,
            observed_at,
        ),
        crate::model::concrete::rejections::authority::use_error(before, command).is_none(),
    ensures crate::model::concrete::rejections::authority::concrete_use_decision(
        before,
        LeaseUseOutcome::Accepted(*accepted),
        command,
    ),
{
    crate::model::concrete::identity::identifiers_matching_common_identity_match(
        accepted.capability_use.spec_scope_environment_id(),
        before.scope.environment.spec_bytes(),
        accepted.claim.scope.environment.spec_bytes(),
    );
    crate::model::concrete::identity::identifiers_matching_common_identity_match(
        accepted
            .capability_use
            .spec_scope_revision()
            .spec_workspace_id()
            .spec_bytes(),
        before.scope.workspace.spec_bytes(),
        accepted.claim.scope.workspace.spec_bytes(),
    );
    crate::model::concrete::identity::identifiers_matching_common_identity_match(
        accepted.capability_use.spec_permission_resource_id(),
        before.scope.resource.spec_bytes(),
        accepted.claim.scope.resource.spec_bytes(),
    );
    assert(crate::model::concrete_policy_intersection(before, command));
    assert(crate::model::concrete_claim_is_current(before, accepted.claim));
    assert(crate::model::concrete_claim_is_current(
        &accepted.lease.next,
        accepted.claim,
    ));
    assert(crate::model::concrete_instant_matches(
        accepted.capability_use.spec_used_at(),
        command.observed_at,
    ));
    assert(accepted.capability_use.spec_used_at().spec_epoch()
        == accepted.claim.expires_at.spec_epoch());
    assert(accepted.capability_use.spec_used_at().spec_tick_millis()
        < accepted.claim.expires_at.spec_tick_millis());
    assert(accepted
        .capability_use
        .spec_scope_revision()
        .spec_workspace_generation()
        .spec_value()
        == accepted.claim.generation.spec_value());
    assert(crate::model::concrete_lease_use_is_current(accepted));
    assert(accepted.lease.record.binding.matches_use_lease_inputs(
        accepted.lease.record.command_id,
        accepted.claim,
        accepted.capability_use.spec_used_at(),
    ));
    assert(accepted.lease.record.binding.matches_use_transition(accepted));
    assert(crate::model::concrete_use_edge(before, accepted, command_id));
    crate::model::concrete::authority::establish_use_transition(before, accepted, command);
    crate::model::concrete::rejections::authority::establish_use_acceptance(
        before,
        accepted,
        command,
    );
}

const fn use_rejection(
    aggregate: LeaseAggregate,
    error: LeaseError,
    command: UseLease,
) -> (result: LeaseUseOutcome)
    requires crate::model::concrete::rejections::authority::use_error(
        &aggregate,
        &command,
    ) == Some(error),
    ensures
        crate::model::concrete::rejections::authority::concrete_use_decision(
            &aggregate,
            result,
            &command,
        ),
{
    let ghost before = aggregate;
    let ghost attempted = command;
    let lease = rejection(aggregate, error);
    let ghost lease_view = lease;
    let failure = LeaseUseFailure::new(lease, command);
    proof {
        crate::model::concrete::authority::establish_use_rejection(
            &before,
            &lease_view,
            &failure,
            &attempted,
            error,
        );
        crate::model::concrete::rejections::authority::establish_use_rejection(
            &before,
            &failure,
            &attempted,
            error,
        );
    }
    LeaseUseOutcome::Rejected(failure)
}

} // verus!
