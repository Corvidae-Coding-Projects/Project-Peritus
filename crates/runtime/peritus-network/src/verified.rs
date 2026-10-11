//! Executable network refinement predicates proved over scalar projections.

#[doc = " Checks the C3 network non-broadening implication."]
#[must_use]
pub const fn network_decision_no_broader(request_allowed: bool, runtime_allowed: bool) -> bool {
    !runtime_allowed || request_allowed
}
#[doc = " Checks a non-wrapping byte charge against its exact ceiling."]
#[must_use]
pub const fn network_charge_allowed(used: u64, charge: u64, limit: u64) -> bool {
    match used.checked_add(charge) {
        Some(total) => total <= limit,
        None => false,
    }
}
