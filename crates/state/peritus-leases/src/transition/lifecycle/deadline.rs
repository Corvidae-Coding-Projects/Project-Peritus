//! Checked lease-deadline arithmetic and its exact refinement contract.

use super::super::map_policy_time;
use crate::{LeaseDuration, LeaseError};
use peritus_policy::AuthorityInstant;
use vstd::prelude::*;

verus! {

pub(super) const fn lease_deadline(
    observed_at: AuthorityInstant,
    duration: LeaseDuration,
) -> (result: Result<AuthorityInstant, LeaseError>)
    ensures
        match result {
            Ok(deadline) => {
                deadline.spec_epoch() == observed_at.spec_epoch()
                    && deadline.spec_tick_millis()
                        == observed_at.spec_tick_millis() + duration.spec_millis()
            }
            Err(error) => {
                error == LeaseError::TimeOverflow
                    && observed_at.spec_tick_millis() + duration.spec_millis()
                        > u64::MAX as int
            }
        },
{
    let duration_millis = duration.millis();
    match observed_at.checked_add(duration_millis) {
        Ok(deadline) => {
            assert(deadline.spec_epoch() == observed_at.spec_epoch());
            assert(deadline.spec_tick_millis()
                == observed_at.spec_tick_millis() + duration.spec_millis());
            Ok(deadline)
        }
        Err(error) => {
            assert(error.spec_kind() == peritus_policy::PolicyErrorKind::TimeOverflow);
            assert(observed_at.spec_tick_millis() + duration.spec_millis()
                > u64::MAX as int);
            let mapped = map_policy_time(error);
            assert(mapped == LeaseError::TimeOverflow);
            Err(mapped)
        }
    }
}

} // verus!
