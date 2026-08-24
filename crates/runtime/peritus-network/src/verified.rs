//! Executable network refinement predicates proved over scalar projections.

use vstd::prelude::*;

verus! {

/// OBL-0131: a runtime allow cannot exist without checked request authority.
pub open spec fn network_decision_no_broader_spec(
    request_allowed: bool,
    runtime_allowed: bool,
) -> bool {
    !runtime_allowed || request_allowed
}

/// Checks the C3 network non-broadening implication.
#[must_use]
pub const fn network_decision_no_broader(
    request_allowed: bool,
    runtime_allowed: bool,
) -> (result: bool)
    ensures result == network_decision_no_broader_spec(request_allowed, runtime_allowed),
{
    !runtime_allowed || request_allowed
}

/// Checks a non-wrapping byte charge against its exact ceiling.
#[must_use]
pub const fn network_charge_allowed(used: u64, charge: u64, limit: u64) -> (result: bool)
    ensures result == (used as int + charge as int <= limit as int),
{
    match used.checked_add(charge) {
        Some(total) => total <= limit,
        None => false,
    }
}

} // verus!
