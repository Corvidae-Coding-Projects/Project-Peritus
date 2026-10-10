//! Named C3 managed-network refinement obligation.

/// Named C3 executable network non-broadening predicate.
#[must_use]
pub const fn network_decision_no_broader(request_allowed: bool, runtime_allowed: bool) -> bool {
    crate::verified::network_decision_no_broader(request_allowed, runtime_allowed)
}
