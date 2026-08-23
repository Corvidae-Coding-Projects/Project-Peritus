//! Exact total decision relations for every executable lease reducer.

pub mod lifecycle;
pub mod authority;
pub mod fencing;
pub mod reconciliation;

use vstd::prelude::*;

verus! {

/// Exact rejected transition: one deterministic error and the complete unchanged aggregate.
pub(crate) open spec fn exact_transition_rejection(
    before: &crate::LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
    error: crate::LeaseError,
) -> bool {
    failure.spec_error() == error
        && super::preservation::concrete_rejection_preserves_input(before, failure)
}

/// Exact total transition decision selected by a command-specific first-error function.
pub closed spec fn exact_transition_decision(
    before: &crate::LeaseAggregate,
    result: crate::LeaseTransitionOutcome,
    expected_error: Option<crate::LeaseError>,
    accepted: spec_fn(&crate::LeaseTransition) -> bool,
) -> bool {
    match (expected_error, result) {
        (None, crate::LeaseTransitionOutcome::Accepted(transition)) => accepted(&transition),
        (Some(error), crate::LeaseTransitionOutcome::Rejected(failure)) => {
            exact_transition_rejection(before, &failure, error)
        }
        _ => false,
    }
}

} // verus!
