//! Exact equality for the copyable lease command families.

use super::super::aggregate::{claims_equal, correlations_equal, dispositions_equal};

use super::super::bytes16_equal;

use crate::{
    AcquireLease, ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, MintLease, ReconcileLease,
    ReleaseLease, RenewLease, RevokeLease,
};

const fn instants_equal(
    left: peritus_policy::AuthorityInstant,
    right: peritus_policy::AuthorityInstant,
) -> bool {
    left.epoch().get() == right.epoch().get() && left.tick_millis() == right.tick_millis()
}
const fn durations_equal(left: crate::LeaseDuration, right: crate::LeaseDuration) -> bool {
    left.millis() == right.millis()
}
pub(super) fn mints_equal(left: MintLease, right: MintLease) -> bool {
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && super::super::aggregate::scopes_equal(left.scope, right.scope)
        && instants_equal(left.observed_at, right.observed_at)
}
pub(super) fn acquires_equal(left: AcquireLease, right: AcquireLease) -> bool {
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && super::super::aggregate::holders_equal(left.holder, right.holder)
        && durations_equal(left.duration, right.duration)
        && instants_equal(left.observed_at, right.observed_at)
}
pub(super) fn renews_equal(left: RenewLease, right: RenewLease) -> bool {
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && claims_equal(left.claim, right.claim)
        && durations_equal(left.duration, right.duration)
        && instants_equal(left.observed_at, right.observed_at)
}
pub(super) fn releases_equal(left: &ReleaseLease, right: &ReleaseLease) -> bool {
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && claims_equal(left.claim, right.claim)
        && instants_equal(left.observed_at, right.observed_at)
        && match (left.quiescence, right.quiescence) {
            (None, None) => true,
            (Some(left_evidence), Some(right_evidence)) => {
                claims_equal(left_evidence.claim(), right_evidence.claim())
                    && bytes16_equal(
                        *left_evidence.evidence_id().as_bytes(),
                        *right_evidence.evidence_id().as_bytes(),
                    )
            }
            _ => false,
        }
}
pub(super) fn expires_equal(left: ExpireLease, right: ExpireLease) -> bool {
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && instants_equal(left.observed_at, right.observed_at)
}
pub(super) fn holder_losses_equal(left: FenceHolderLoss, right: FenceHolderLoss) -> bool {
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && instants_equal(left.observed_at, right.observed_at)
        && claims_equal(left.evidence.claim(), right.evidence.claim())
        && bytes16_equal(
            *left.evidence.evidence_id().as_bytes(),
            *right.evidence.evidence_id().as_bytes(),
        )
}
pub(super) fn discontinuities_equal(
    left: FenceClockDiscontinuity,
    right: FenceClockDiscontinuity,
) -> bool {
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && instants_equal(left.observed_at, right.observed_at)
}
pub(super) fn revokes_equal(left: RevokeLease, right: RevokeLease) -> bool {
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && claims_equal(left.claim, right.claim)
        && instants_equal(left.observed_at, right.observed_at)
        && bytes16_equal(*left.evidence_id.as_bytes(), *right.evidence_id.as_bytes())
}
pub(super) fn reconciles_equal(left: ReconcileLease, right: ReconcileLease) -> bool {
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && instants_equal(left.observed_at, right.observed_at)
        && correlations_equal(left.observation.correlation, right.observation.correlation)
        && dispositions_equal(left.observation.disposition, right.observation.disposition)
}
