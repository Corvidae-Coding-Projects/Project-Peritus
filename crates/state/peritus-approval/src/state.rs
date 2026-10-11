//! Move-only approval aggregate and total checked reducers.

pub mod exact;
mod independence;
mod lifecycle;
mod lifecycle_specification;
mod resolve_helpers;
mod resolve_specification;
mod specification;
mod types;
mod use_reducers;
mod use_specification;
mod validation;

use resolve_helpers::{
    advance_time_checked, existing_resolution, observation_expiry, transition_failure,
};
pub use types::{
    ApprovalAggregate, ApprovalResolutionFacts, ApprovalTransition, ApprovalTransitionKind,
    ApprovalTransitionOutcome,
};
use types::{ApprovalState, Resolution};

#[allow(
    non_shorthand_field_patterns,
    reason = "pinned Verus expands move-only destructures to explicit field patterns"
)]
impl ApprovalAggregate {
    #[doc = " Creates a pending aggregate from one checked request."]
    #[must_use]
    pub const fn new(request: crate::ApprovalRequest) -> Self {
        let aggregate = Self { request, state: ApprovalState::Pending };
        aggregate
    }
}
#[allow(
    non_shorthand_field_patterns,
    reason = "pinned Verus expands move-only destructures to explicit field patterns"
)]
impl ApprovalAggregate {
    #[doc = " Resolves a pending request from one checked authentication observation."]
    #[doc = ""]
    #[doc = " Exact response replay is idempotent. A conflicting response cannot change terminal state."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a move-only rejection preserving the unchanged aggregate and observation."]
    #[allow(
        clippy::result_large_err,
        reason = "rejection must own the unchanged aggregate and authentication observation"
    )]
    #[allow(
        clippy::single_match_else,
        clippy::too_many_lines,
        reason = "explicit state matches expose refinement facts to pinned Verus in one atomic reducer"
    )]
    pub fn resolve(
        self,
        observation: crate::AuthenticatedApprovalObservation,
        registry: &crate::CredentialRegistrySnapshot,
    ) -> Result<ApprovalTransitionOutcome, crate::ApprovalTransitionFailure> {
        let result = self.resolve_checked(observation, registry);
        result
    }
    #[allow(
        clippy::result_large_err,
        reason = "rejection must own the unchanged aggregate and authentication observation"
    )]
    #[allow(
        clippy::single_match_else,
        clippy::too_many_lines,
        reason = "explicit state matches expose exact refinement facts in one atomic reducer"
    )]
    fn resolve_checked(
        self,
        observation: crate::AuthenticatedApprovalObservation,
        registry: &crate::CredentialRegistrySnapshot,
    ) -> Result<ApprovalTransitionOutcome, crate::ApprovalTransitionFailure> {
        let existing = existing_resolution(self.state);
        match self.state {
            ApprovalState::Pending => {}
            _ => {
                let Some(value) = existing else {
                    return Err(transition_failure(
                        crate::ApprovalError::AlreadyResolved,
                        self,
                        Some(observation),
                    ));
                };
                if !exact::digest_bytes_equal(
                    *value.decision_digest.sha256().as_bytes(),
                    *observation.decision_digest.sha256().as_bytes(),
                ) {
                    return Err(transition_failure(
                        crate::ApprovalError::AlreadyResolved,
                        self,
                        Some(observation),
                    ));
                }
            }
        }
        if let Err(error) = validation::checked_observation(&self.request, &observation, registry) {
            return Err(transition_failure(error, self, Some(observation)));
        }
        match self.state {
            ApprovalState::Pending => {}
            _ => {
                let phase = self.phase();
                let outcome = ApprovalTransitionOutcome {
                    aggregate: self,
                    transition: ApprovalTransition {
                        kind: ApprovalTransitionKind::Idempotent,
                        from: phase,
                        to: phase,
                        decision_digest: Some(observation.decision_digest),
                        registry_revision: Some(observation.registry_revision),
                    },
                };
                return Ok(outcome);
            }
        }
        let valid_until = match observation_expiry(&self.request, &observation) {
            Ok(value) => value,
            Err(error) => return Err(transition_failure(error, self, Some(observation))),
        };
        if let Err(error) = self.request.validate_observation_time(observation.observed_at) {
            return Err(transition_failure(error, self, Some(observation)));
        }
        if observation.observed_at.tick_millis() >= valid_until.tick_millis() {
            return Err(transition_failure(crate::ApprovalError::Expired, self, Some(observation)));
        }
        let Self { request: request, state: _ } = self;
        let request = advance_time_checked(request, observation.observed_at);
        let resolution = Resolution {
            decision_digest: observation.decision_digest,
            command_id: observation.command_id,
            choice: observation.choice,
            registry_revision: observation.registry_revision,
            registry_digest: observation.registry_digest,
            credential_generation: observation.credential_generation,
            valid_until,
        };
        let next_state = match observation.choice {
            crate::ApprovalChoice::Deny => ApprovalState::Denied(resolution),
            crate::ApprovalChoice::ApproveOnce => ApprovalState::ApprovedOnce(resolution),
            crate::ApprovalChoice::Amend(_) => ApprovalState::AmendmentAuthorized(resolution),
        };
        let aggregate = Self { request, state: next_state };
        let to = match observation.choice {
            crate::ApprovalChoice::Deny => crate::ApprovalPhase::Denied,
            crate::ApprovalChoice::ApproveOnce => crate::ApprovalPhase::ApprovedOnce,
            crate::ApprovalChoice::Amend(_) => crate::ApprovalPhase::AmendmentAuthorized,
        };
        let outcome = ApprovalTransitionOutcome {
            aggregate,
            transition: ApprovalTransition {
                kind: ApprovalTransitionKind::Resolved,
                from: crate::ApprovalPhase::Pending,
                to,
                decision_digest: Some(observation.decision_digest),
                registry_revision: Some(observation.registry_revision),
            },
        };
        Ok(outcome)
    }
    #[doc = " Cancels only an unresolved pending request."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the unchanged aggregate when it is no longer pending."]
    #[allow(
        clippy::result_large_err,
        reason = "rejection must own the unchanged move-only aggregate"
    )]
    #[allow(
        clippy::single_match_else,
        reason = "explicit state matching exposes the pending refinement fact to pinned Verus"
    )]
    pub fn cancel(self) -> Result<ApprovalTransitionOutcome, crate::ApprovalTransitionFailure> {
        let result = self.cancel_checked();
        result
    }
    #[allow(
        clippy::result_large_err,
        reason = "exact rejection owns the unchanged move-only aggregate"
    )]
    fn cancel_checked(self) -> Result<ApprovalTransitionOutcome, crate::ApprovalTransitionFailure> {
        if !matches!(self.state, ApprovalState::Pending) {
            let actual = self.phase();
            return Err(transition_failure(
                crate::ApprovalError::IllegalPhase {
                    expected: crate::ApprovalPhase::Pending,
                    actual,
                },
                self,
                None,
            ));
        }
        let aggregate = Self { request: self.request, state: ApprovalState::Cancelled };
        let outcome = ApprovalTransitionOutcome {
            aggregate,
            transition: ApprovalTransition {
                kind: ApprovalTransitionKind::Cancelled,
                from: crate::ApprovalPhase::Pending,
                to: crate::ApprovalPhase::Cancelled,
                decision_digest: None,
                registry_revision: None,
            },
        };
        Ok(outcome)
    }
}
