//! Exact equality for the copyable lease command families.

use super::super::aggregate::{claims_equal, correlations_equal, dispositions_equal};
#[cfg(verus_only)]
use super::super::aggregate::{
    claim_fields_match, correlation_fields_match, disposition_fields_match,
    holder_fields_match, scope_fields_match,
};
use super::super::bytes16_equal;
#[cfg(verus_only)]
use super::super::bytes16_match;
use crate::{
    AcquireLease, ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, MintLease,
    ReconcileLease, ReleaseLease, RenewLease, RevokeLease,
};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn instant_fields_match(
    left: peritus_policy::AuthorityInstant,
    right: peritus_policy::AuthorityInstant,
) -> bool {
    crate::model::concrete_instant_matches(left, right)
}

const fn instants_equal(
    left: peritus_policy::AuthorityInstant,
    right: peritus_policy::AuthorityInstant,
) -> (equal: bool)
    ensures equal == instant_fields_match(left, right),
{
    left.epoch().get() == right.epoch().get()
        && left.tick_millis() == right.tick_millis()
}

pub(crate) open spec fn duration_fields_match(
    left: crate::LeaseDuration,
    right: crate::LeaseDuration,
) -> bool {
    left.spec_millis() == right.spec_millis()
}

const fn durations_equal(
    left: crate::LeaseDuration,
    right: crate::LeaseDuration,
) -> (equal: bool)
    ensures equal == duration_fields_match(left, right),
{
    left.millis() == right.millis()
}

pub(crate) open spec fn mint_fields_match(left: MintLease, right: MintLease) -> bool {
    bytes16_match(left.command_id.spec_bytes(), right.command_id.spec_bytes())
        && scope_fields_match(left.scope, right.scope)
        && instant_fields_match(left.observed_at, right.observed_at)
}

pub(super) fn mints_equal(left: MintLease, right: MintLease) -> (equal: bool)
    ensures equal == mint_fields_match(left, right),
{
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && super::super::aggregate::scopes_equal(left.scope, right.scope)
        && instants_equal(left.observed_at, right.observed_at)
}

pub(crate) open spec fn acquire_fields_match(
    left: AcquireLease,
    right: AcquireLease,
) -> bool {
    bytes16_match(left.command_id.spec_bytes(), right.command_id.spec_bytes())
        && holder_fields_match(left.holder, right.holder)
        && duration_fields_match(left.duration, right.duration)
        && instant_fields_match(left.observed_at, right.observed_at)
}

pub(super) fn acquires_equal(left: AcquireLease, right: AcquireLease) -> (equal: bool)
    ensures equal == acquire_fields_match(left, right),
{
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && super::super::aggregate::holders_equal(left.holder, right.holder)
        && durations_equal(left.duration, right.duration)
        && instants_equal(left.observed_at, right.observed_at)
}

pub(crate) open spec fn renew_fields_match(left: RenewLease, right: RenewLease) -> bool {
    bytes16_match(left.command_id.spec_bytes(), right.command_id.spec_bytes())
        && claim_fields_match(left.claim, right.claim)
        && duration_fields_match(left.duration, right.duration)
        && instant_fields_match(left.observed_at, right.observed_at)
}

pub(super) fn renews_equal(left: RenewLease, right: RenewLease) -> (equal: bool)
    ensures equal == renew_fields_match(left, right),
{
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && claims_equal(left.claim, right.claim)
        && durations_equal(left.duration, right.duration)
        && instants_equal(left.observed_at, right.observed_at)
}

pub(crate) open spec fn release_fields_match(left: ReleaseLease, right: ReleaseLease) -> bool {
    bytes16_match(left.command_id.spec_bytes(), right.command_id.spec_bytes())
        && claim_fields_match(left.claim, right.claim)
        && instant_fields_match(left.observed_at, right.observed_at)
        && match (left.quiescence, right.quiescence) {
            (None, None) => true,
            (Some(left_evidence), Some(right_evidence)) => {
                claim_fields_match(
                    left_evidence.spec_claim(),
                    right_evidence.spec_claim(),
                ) && bytes16_match(
                    left_evidence.spec_evidence_id().spec_bytes(),
                    right_evidence.spec_evidence_id().spec_bytes(),
                )
            }
            _ => false,
        }
}

pub(super) fn releases_equal(left: &ReleaseLease, right: &ReleaseLease) -> (equal: bool)
    ensures equal == release_fields_match(*left, *right),
{
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

pub(crate) open spec fn expire_fields_match(left: ExpireLease, right: ExpireLease) -> bool {
    bytes16_match(left.command_id.spec_bytes(), right.command_id.spec_bytes())
        && instant_fields_match(left.observed_at, right.observed_at)
}

pub(super) fn expires_equal(left: ExpireLease, right: ExpireLease) -> (equal: bool)
    ensures equal == expire_fields_match(left, right),
{
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && instants_equal(left.observed_at, right.observed_at)
}

pub(crate) open spec fn holder_loss_fields_match(
    left: FenceHolderLoss,
    right: FenceHolderLoss,
) -> bool {
    bytes16_match(left.command_id.spec_bytes(), right.command_id.spec_bytes())
        && instant_fields_match(left.observed_at, right.observed_at)
        && claim_fields_match(left.evidence.spec_claim(), right.evidence.spec_claim())
        && bytes16_match(
            left.evidence.spec_evidence_id().spec_bytes(),
            right.evidence.spec_evidence_id().spec_bytes(),
        )
}

pub(super) fn holder_losses_equal(
    left: FenceHolderLoss,
    right: FenceHolderLoss,
) -> (equal: bool)
    ensures equal == holder_loss_fields_match(left, right),
{
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && instants_equal(left.observed_at, right.observed_at)
        && claims_equal(left.evidence.claim(), right.evidence.claim())
        && bytes16_equal(
            *left.evidence.evidence_id().as_bytes(),
            *right.evidence.evidence_id().as_bytes(),
        )
}

pub(crate) open spec fn discontinuity_fields_match(
    left: FenceClockDiscontinuity,
    right: FenceClockDiscontinuity,
) -> bool {
    bytes16_match(left.command_id.spec_bytes(), right.command_id.spec_bytes())
        && instant_fields_match(left.observed_at, right.observed_at)
}

pub(super) fn discontinuities_equal(
    left: FenceClockDiscontinuity,
    right: FenceClockDiscontinuity,
) -> (equal: bool)
    ensures equal == discontinuity_fields_match(left, right),
{
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && instants_equal(left.observed_at, right.observed_at)
}

pub(crate) open spec fn revoke_fields_match(left: RevokeLease, right: RevokeLease) -> bool {
    bytes16_match(left.command_id.spec_bytes(), right.command_id.spec_bytes())
        && claim_fields_match(left.claim, right.claim)
        && instant_fields_match(left.observed_at, right.observed_at)
        && bytes16_match(left.evidence_id.spec_bytes(), right.evidence_id.spec_bytes())
}

pub(super) fn revokes_equal(left: RevokeLease, right: RevokeLease) -> (equal: bool)
    ensures equal == revoke_fields_match(left, right),
{
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && claims_equal(left.claim, right.claim)
        && instants_equal(left.observed_at, right.observed_at)
        && bytes16_equal(*left.evidence_id.as_bytes(), *right.evidence_id.as_bytes())
}

pub(crate) open spec fn reconcile_fields_match(
    left: ReconcileLease,
    right: ReconcileLease,
) -> bool {
    bytes16_match(left.command_id.spec_bytes(), right.command_id.spec_bytes())
        && instant_fields_match(left.observed_at, right.observed_at)
        && correlation_fields_match(
            left.observation.correlation,
            right.observation.correlation,
        )
        && disposition_fields_match(
            left.observation.disposition,
            right.observation.disposition,
        )
}

pub(super) fn reconciles_equal(left: ReconcileLease, right: ReconcileLease) -> (equal: bool)
    ensures equal == reconcile_fields_match(left, right),
{
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && instants_equal(left.observed_at, right.observed_at)
        && correlations_equal(left.observation.correlation, right.observation.correlation)
        && dispositions_equal(left.observation.disposition, right.observation.disposition)
}

} // verus!
