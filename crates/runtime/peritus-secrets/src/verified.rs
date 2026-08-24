//! Executable exact-delivery predicate proved with Verus.

use vstd::prelude::*;

verus! {

/// OBL-0132 exact live lease implication.
pub open spec fn secret_delivery_exact_spec(
    reference_matches: bool,
    destination_matches: bool,
    lease_live: bool,
) -> bool {
    reference_matches && destination_matches && lease_live
}

/// Checks exact reference, destination, and live lease facts.
#[must_use]
pub const fn secret_delivery_exact(
    reference_matches: bool,
    destination_matches: bool,
    lease_live: bool,
) -> (result: bool)
    ensures result == secret_delivery_exact_spec(reference_matches, destination_matches, lease_live),
{
    reference_matches && destination_matches && lease_live
}

} // verus!
