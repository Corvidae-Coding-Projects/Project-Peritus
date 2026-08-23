//! Exact internal first-error and authority-time model for fencing application.

#[cfg(verus_only)]
use super::{AuthorityTimeAdvance, LeaseAggregate, LeaseError, LeaseState, LeaseTransitionKind};
#[cfg(verus_only)]
use crate::FenceCause;
#[cfg(verus_only)]
use peritus_policy::AuthorityInstant;
use vstd::prelude::*;

verus! {

pub(super) open spec fn fence_time_advance_matches(
    before: &LeaseAggregate,
    advance: AuthorityTimeAdvance,
    observed_at: AuthorityInstant,
    cause: Option<FenceCause>,
) -> bool {
    match cause {
        Some(FenceCause::ClockDiscontinuity) => {
            if observed_at.spec_epoch() == before.authority_time.spec_epoch() {
                advance == AuthorityTimeAdvance::Preserve
                    && observed_at.spec_tick_millis()
                        < before.authority_time.spec_greatest_tick_millis()
            } else {
                advance == AuthorityTimeAdvance::Reset(observed_at)
            }
        }
        None
        | Some(
            FenceCause::ReleasedWithoutQuiescence
            | FenceCause::Expired
            | FenceCause::HolderLost
            | FenceCause::Revoked,
        ) => {
            advance == AuthorityTimeAdvance::Observe(observed_at)
                && super::validation::observation_error(
                    &before.authority_time,
                    observed_at,
                ).is_none()
        }
    }
}

pub(super) proof fn transition_error_is_final_fence_error(
    before: &LeaseAggregate,
    version: peritus_types::RevisionNumber,
    advance: AuthorityTimeAdvance,
    observed_at: AuthorityInstant,
    cause: Option<FenceCause>,
)
    requires
        version.spec_value() == before.version.spec_value() + 1,
        before.version.spec_value() < u64::MAX as int,
        fence_time_advance_matches(before, advance, observed_at, cause),
    ensures super::core::transition_error(before, version, advance)
        == crate::model::concrete::rejections::fencing::final_fence_error(before),
{
    match cause {
        Some(FenceCause::ClockDiscontinuity) => {
            if observed_at.spec_epoch() == before.authority_time.spec_epoch() {
                assert(advance == AuthorityTimeAdvance::Preserve);
            } else {
                assert(advance == AuthorityTimeAdvance::Reset(observed_at));
            }
        }
        None
        | Some(
            FenceCause::ReleasedWithoutQuiescence
            | FenceCause::Expired
            | FenceCause::HolderLost
            | FenceCause::Revoked,
        ) => {
            assert(advance == AuthorityTimeAdvance::Observe(observed_at));
        }
    }
}

pub(super) open spec fn fence_error(
    before: &LeaseAggregate,
    observed_at: AuthorityInstant,
    cause: Option<FenceCause>,
    kind: LeaseTransitionKind,
) -> Option<LeaseError> {
    if !matches!(before.state, LeaseState::Active(_))
        || !crate::model::concrete_fencing_kind(kind)
    {
        Some(LeaseError::CorruptState)
    } else {
        match cause {
            Some(FenceCause::ClockDiscontinuity) => {
                if observed_at.spec_epoch() == before.authority_time.spec_epoch()
                    && observed_at.spec_tick_millis()
                        >= before.authority_time.spec_greatest_tick_millis()
                {
                    Some(LeaseError::NoClockDiscontinuity)
                } else {
                    crate::model::concrete::rejections::fencing::final_fence_error(before)
                }
            }
            None
            | Some(
                FenceCause::ReleasedWithoutQuiescence
                | FenceCause::Expired
                | FenceCause::HolderLost
                | FenceCause::Revoked,
            ) => {
                let observation = super::validation::observation_error(
                    &before.authority_time,
                    observed_at,
                );
                if observation.is_some() {
                    observation
                } else {
                    crate::model::concrete::rejections::fencing::final_fence_error(before)
                }
            }
        }
    }
}

} // verus!
