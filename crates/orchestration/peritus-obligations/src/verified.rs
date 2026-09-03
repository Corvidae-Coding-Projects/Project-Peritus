//! Verus specification and proofs for obligation qualification and failure routing.

use crate::FailureOwner;
use vstd::prelude::*;

verus! {

/// Acceptance relation for current required evidence, complete alternatives, and resolved
/// conditions.
pub open spec fn qualification_allowed_spec(
    required_obligations_current: bool,
    alternatives_complete: bool,
    conditions_resolved: bool,
) -> bool {
    required_obligations_current && alternatives_complete && conditions_resolved
}

/// A conditional obligation becomes active exactly when its public condition holds.
pub open spec fn conditional_obligation_active_spec(condition_holds: bool) -> bool {
    condition_holds
}

/// An alternative group is complete exactly when at least one entire branch is complete.
pub open spec fn alternative_group_complete_spec(some_complete_branch: bool) -> bool {
    some_complete_branch
}

/// Only candidate-owned defects may authorize another fixer cycle.
pub open spec fn failure_authorizes_fixer_spec(owner: FailureOwner) -> bool {
    owner == FailureOwner::CandidateDefect
}

/// Executable qualification predicate corresponding to the Verus qualification specification.
#[must_use]
pub const fn qualification_allowed(
    required_obligations_current: bool,
    alternatives_complete: bool,
    conditions_resolved: bool,
) -> (result: bool)
    ensures result == qualification_allowed_spec(
        required_obligations_current,
        alternatives_complete,
        conditions_resolved,
    ),
{
    required_obligations_current && alternatives_complete && conditions_resolved
}

/// Executable conditional-activation predicate.
#[must_use]
pub const fn conditional_obligation_active(condition_holds: bool) -> (result: bool)
    ensures result == conditional_obligation_active_spec(condition_holds),
{
    condition_holds
}

/// Executable alternative-completeness predicate.
#[must_use]
pub const fn alternative_group_complete(some_complete_branch: bool) -> (result: bool)
    ensures result == alternative_group_complete_spec(some_complete_branch),
{
    some_complete_branch
}

/// Executable failure-to-fixer authorization predicate.
#[must_use]
pub const fn failure_authorizes_fixer(owner: FailureOwner) -> (result: bool)
    ensures result == failure_authorizes_fixer_spec(owner),
{
    matches!(owner, FailureOwner::CandidateDefect)
}

/// Qualification implies every ordinary required obligation has current satisfying evidence.
pub proof fn qualified_requires_current_evidence(
    required_obligations_current: bool,
    alternatives_complete: bool,
    conditions_resolved: bool,
)
    requires qualification_allowed_spec(
        required_obligations_current,
        alternatives_complete,
        conditions_resolved,
    ),
    ensures required_obligations_current,
{}

/// Qualification implies at least one complete branch in every alternative group.
pub proof fn qualified_requires_complete_alternatives(
    required_obligations_current: bool,
    alternatives_complete: bool,
    conditions_resolved: bool,
)
    requires qualification_allowed_spec(
        required_obligations_current,
        alternatives_complete,
        conditions_resolved,
    ),
    ensures alternative_group_complete_spec(alternatives_complete),
{}

/// A false public condition cannot activate its conditional obligation.
pub proof fn conditional_activation_requires_public_truth(condition_holds: bool)
    ensures conditional_obligation_active_spec(condition_holds) ==> condition_holds,
{}

/// A non-candidate failure owner cannot authorize a fixer transition.
pub proof fn non_candidate_failure_cannot_authorize_fixer(owner: FailureOwner)
    requires owner != FailureOwner::CandidateDefect,
    ensures !failure_authorizes_fixer_spec(owner),
{}

} // verus!
