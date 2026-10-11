//! Checked lease-deadline arithmetic and its exact refinement contract.

use super::super::map_policy_time;
use crate::{LeaseDuration, LeaseError};
use peritus_policy::AuthorityInstant;

pub(super) const fn lease_deadline(
    observed_at: AuthorityInstant,
    duration: LeaseDuration,
) -> Result<AuthorityInstant, LeaseError> {
    let duration_millis = duration.millis();
    match observed_at.checked_add(duration_millis) {
        Ok(deadline) => Ok(deadline),
        Err(error) => {
            let mapped = map_policy_time(error);
            Err(mapped)
        }
    }
}
