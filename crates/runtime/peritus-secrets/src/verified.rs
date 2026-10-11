//! Executable exact-delivery predicate proved with Verus.

#[doc = " Checks exact reference, destination, and live lease facts."]
#[must_use]
pub const fn secret_delivery_exact(
    reference_matches: bool,
    destination_matches: bool,
    lease_live: bool,
) -> bool {
    reference_matches && destination_matches && lease_live
}
