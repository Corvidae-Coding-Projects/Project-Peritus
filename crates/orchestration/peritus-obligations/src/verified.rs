//! Verus specification and proofs for obligation qualification and failure routing.

use crate::FailureOwner;

#[doc = " Executable qualification predicate corresponding to the Verus qualification specification."]
#[must_use]
pub const fn qualification_allowed(
    required_obligations_current: bool,
    alternatives_complete: bool,
    conditions_resolved: bool,
) -> bool {
    required_obligations_current && alternatives_complete && conditions_resolved
}
#[doc = " Executable conditional-activation predicate."]
#[must_use]
pub const fn conditional_obligation_active(condition_holds: bool) -> bool {
    condition_holds
}
#[doc = " Executable alternative-completeness predicate."]
#[must_use]
pub const fn alternative_group_complete(some_complete_branch: bool) -> bool {
    some_complete_branch
}
#[doc = " Executable failure-to-fixer authorization predicate."]
#[must_use]
pub const fn failure_authorizes_fixer(owner: FailureOwner) -> bool {
    matches!(owner, FailureOwner::CandidateDefect)
}
