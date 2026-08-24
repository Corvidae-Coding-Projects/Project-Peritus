//! Named C3 exact secret-delivery refinement obligation.

use vstd::prelude::*;

/// Named C3 executable exact-lease predicate.
#[must_use]
pub const fn secret_delivery_exact(
    reference_matches: bool,
    destination_matches: bool,
    lease_live: bool,
) -> bool {
    crate::verified::secret_delivery_exact(reference_matches, destination_matches, lease_live)
}

verus! {

/// `OBL-0132`: every delivery uses an exact live lease for the authorized destination.
pub proof fn delivered_secret_implies_exact_live_lease(
    reference_matches: bool,
    destination_matches: bool,
    lease_live: bool,
)
    requires crate::verified::secret_delivery_exact_spec(
        reference_matches,
        destination_matches,
        lease_live,
    ),
    ensures reference_matches, destination_matches, lease_live,
{
}

} // verus!
