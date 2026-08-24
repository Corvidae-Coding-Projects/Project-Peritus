//! Named C3 managed-network refinement obligation.

use vstd::prelude::*;

/// Named C3 executable network non-broadening predicate.
#[must_use]
pub const fn network_decision_no_broader(request_allowed: bool, runtime_allowed: bool) -> bool {
    crate::verified::network_decision_no_broader(request_allowed, runtime_allowed)
}

verus! {

/// `OBL-0131`: a runtime allow is never broader than checked network authority.
pub proof fn managed_network_allow_implies_checked_allow(
    checked_allowed: bool,
    runtime_allowed: bool,
)
    requires
        crate::verified::network_decision_no_broader_spec(
            checked_allowed,
            runtime_allowed,
        ),
        runtime_allowed,
    ensures checked_allowed,
{
}

} // verus!
