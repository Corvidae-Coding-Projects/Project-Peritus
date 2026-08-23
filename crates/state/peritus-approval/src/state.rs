//! Move-only approval aggregate and total checked reducers.

use vstd::prelude::*;

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

verus! {

#[allow(
    non_shorthand_field_patterns,
    reason = "pinned Verus expands move-only destructures to explicit field patterns"
)]
impl ApprovalAggregate {
    /// Creates a pending aggregate from one checked request.
    #[must_use]
    pub const fn new(request: crate::ApprovalRequest) -> (aggregate: Self)
        ensures aggregate.spec_model() == crate::model::initial(),
    {
        let aggregate = Self { request, state: ApprovalState::Pending };
        proof { specification::initial_refines(&aggregate); }
        aggregate
    }

}

#[allow(
    non_shorthand_field_patterns,
    reason = "pinned Verus expands move-only destructures to explicit field patterns"
)]
impl ApprovalAggregate {
    /// Resolves a pending request from one checked authentication observation.
    ///
    /// Exact response replay is idempotent. A conflicting response cannot change terminal state.
    ///
    /// # Errors
    ///
    /// Returns a move-only rejection preserving the unchanged aggregate and observation.
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
    ) -> (result: Result<ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>)
        ensures self.spec_resolve_result_is_exact(&observation, registry, &result),
    {
        let ghost before = self;
        let ghost exact_observation = observation;
        let result = self.resolve_checked(observation, registry);
        proof {
            resolve_specification::close_resolve_relation(
                &before,
                &exact_observation,
                registry,
                &result,
            );
        }
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
    ) -> (result: Result<ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>)
        ensures resolve_specification::resolve_result_relation(
            &self,
            &observation,
            registry,
            &result,
        ),
    {
        proof { reveal_with_fuel(resolve_specification::resolve_result_relation, 1); }
        let existing = existing_resolution(self.state);
        match self.state {
            ApprovalState::Pending => {},
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
            ApprovalState::Pending => {},
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
                proof {
                    observation.prove_specs();
                    outcome.prove_model();
                    specification::replay_refines(
                        &self,
                        &outcome.aggregate,
                        observation.spec_decision_digest(),
                    );
                    crate::proofs::exact_replay_is_idempotent(
                        self.spec_model(),
                        observation.spec_decision_digest(),
                    );
                    crate::proofs::accepted_reducer_refines(
                        self.spec_model(),
                        crate::model::ApprovalModelStep::Replay(
                            observation.spec_decision_digest(),
                        ),
                        outcome.spec_model(),
                    );
                }
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
        proof { self.request.observation_time_ok(observation.observed_at); }
        if observation.observed_at.tick_millis() >= valid_until.tick_millis() {
            return Err(transition_failure(
                crate::ApprovalError::Expired,
                self,
                Some(observation),
            ));
        }
        let Self { request, state: _ } = self;
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
        proof {
            assert(self.state == ApprovalState::Pending);
            observation.prove_specs();
            outcome.prove_model();
            specification::pending_resolution_refines(
                &self,
                &outcome.aggregate,
                observation.spec_choice(),
                observation.spec_decision_digest(),
                resolution,
            );
            crate::proofs::accepted_reducer_refines(
                self.spec_model(),
                crate::model::resolution_step(
                    observation.spec_choice(),
                    observation.spec_decision_digest(),
                ),
                outcome.spec_model(),
            );
            reveal_with_fuel(resolve_specification::pending_success_is_exact, 1);
            reveal_with_fuel(resolve_specification::exact_resolution, 1);
            reveal_with_fuel(resolve_specification::state_from_resolution, 1);
            assert(resolve_specification::observation_expiry_result(
                &self.request,
                &observation,
            ) == Ok(valid_until));
            assert(resolve_specification::exact_resolution(&observation, valid_until)
                == resolution);
            assert(exact::request_is_exact_advance(
                &outcome.aggregate.request,
                &self.request,
                observation.observed_at,
            ));
            assert(outcome.aggregate.state
                == resolve_specification::state_from_resolution(resolution));
            assert(outcome.transition.kind == ApprovalTransitionKind::Resolved);
            assert(outcome.transition.from == crate::ApprovalPhase::Pending);
            assert(outcome.transition.to == exact::state_phase(
                resolve_specification::state_from_resolution(resolution),
            ));
            assert(outcome.transition.decision_digest == Some(observation.decision_digest));
            assert(outcome.transition.registry_revision == Some(observation.registry_revision));
            assert(crate::model::inv_009(outcome.spec_model()));
            assert(resolve_specification::pending_success_is_exact(
                &self,
                &observation,
                &outcome,
            ));
        }
        Ok(outcome)
    }

    /// Cancels only an unresolved pending request.
    ///
    /// # Errors
    ///
    /// Returns the unchanged aggregate when it is no longer pending.
    #[allow(
        clippy::result_large_err,
        reason = "rejection must own the unchanged move-only aggregate"
    )]
    #[allow(
        clippy::single_match_else,
        reason = "explicit state matching exposes the pending refinement fact to pinned Verus"
    )]
    pub fn cancel(
        self,
    ) -> (result: Result<ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>)
        ensures self.spec_cancel_result_is_exact(&result),
    {
        let ghost before = self;
        let result = self.cancel_checked();
        proof {
            lifecycle_specification::close_cancel_relation(&before, &result);
        }
        result
    }

    #[allow(
        clippy::result_large_err,
        reason = "exact rejection owns the unchanged move-only aggregate"
    )]
    fn cancel_checked(
        self,
    ) -> (result: Result<ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>)
        ensures lifecycle_specification::cancel_result_relation(&self, &result),
    {
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
        proof { assert(self.state == ApprovalState::Pending); }
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
        proof {
            outcome.prove_model();
            specification::cancel_refines(&self, &outcome.aggregate);
            crate::proofs::accepted_reducer_refines(
                self.spec_model(),
                crate::model::ApprovalModelStep::Cancel,
                outcome.spec_model(),
            );
        }
        Ok(outcome)
    }

}

} // verus!
