//! Named C3 exact secret-delivery refinement obligation.

/// Named C3 executable exact-lease predicate.
#[must_use]
pub const fn secret_delivery_exact(
    reference_matches: bool,
    destination_matches: bool,
    lease_live: bool,
) -> bool {
    crate::verified::secret_delivery_exact(reference_matches, destination_matches, lease_live)
}
