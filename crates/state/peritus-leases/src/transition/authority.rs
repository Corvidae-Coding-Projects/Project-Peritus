//! Exact policy-use and current-lease intersection reducer.

use super::authority_validation::validate_policy_intersection;
use super::{
    AuthorityTimeAdvance, LeaseAggregate, LeaseError, LeaseState, LeaseTransitionKind,
    LeaseUseTransition, TransitionPlan, earlier, ensure_before_expiry, next_active_version,
    rejection, require_active_claim, transition, validate_observation,
};
use crate::{LeaseTransitionOutcome, LeaseUseFailure, LeaseUseOutcome, UseLease};

impl LeaseAggregate {
    #[doc = " Intersects a current exact claim with a freshly consumed exact policy capability."]
    #[doc = ""]
    #[doc = " Failure returns no logical lease use and leaves the caller\'s aggregate unchanged."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed claim, time, version, policy-use, or exact authority-intersection failure."]
    pub fn authorize_use(self, command: UseLease) -> LeaseUseOutcome {
        let active = match require_active_claim(&self, command.claim()) {
            Ok(value) => value,
            Err(error) => {
                return use_rejection(self, error, command);
            }
        };
        if let Err(error) = validate_observation(&self.authority_time, command.observed_at()) {
            return use_rejection(self, error, command);
        }
        if let Err(error) = ensure_before_expiry(active.expires_at, command.observed_at()) {
            return use_rejection(self, error, command);
        }
        if let Err(error) = validate_policy_intersection(&self, &command) {
            return use_rejection(self, error, command);
        }
        let version = match next_active_version(self.version) {
            Ok(value) => value,
            Err(error) => {
                return use_rejection(self, error, command);
            }
        };
        let effective_expires_at =
            earlier(active.expires_at, command.capability_use.scope().validity().expires_at());
        let kind = use_transition_kind(&command);
        let command_id = command.command_id();
        let observed_at = command.observed_at();
        let generation = self.generation;
        let _claim = command.claim();
        let binding = crate::LeaseCommandBinding::use_command(&command);
        let lease = match transition(
            self,
            TransitionPlan::new(
                command_id,
                version,
                generation,
                AuthorityTimeAdvance::Observe(observed_at),
                LeaseState::Active(active),
                kind,
                binding,
            ),
        ) {
            LeaseTransitionOutcome::Accepted(lease) => lease,
            LeaseTransitionOutcome::Rejected(failure) => {
                let use_failure = LeaseUseFailure::new(failure, command);
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
        LeaseUseOutcome::Accepted(accepted)
    }
}
const fn use_transition_kind(command: &UseLease) -> LeaseTransitionKind {
    LeaseTransitionKind::Used {
        action_id: command.capability_use.action_id(),
        action_digest: command.capability_use.action_digest(),
    }
}
const fn use_rejection(
    aggregate: LeaseAggregate,
    error: LeaseError,
    command: UseLease,
) -> LeaseUseOutcome {
    let lease = rejection(aggregate, error);
    let failure = LeaseUseFailure::new(lease, command);
    LeaseUseOutcome::Rejected(failure)
}
