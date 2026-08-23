//! Checked read-only projections of private ledger records.

use crate::{
    BudgetAccountPhase, BudgetAmounts, BudgetLimits, ReservationPhase, UsageFinality,
};
use crate::state::{BudgetAccount, ReservationRecord};
use peritus_types::{BudgetId, RevisionTuple, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Checked immutable account projection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetSnapshot {
    id: BudgetId,
    parent_id: Option<BudgetId>,
    revision: RevisionTuple,
    limits: BudgetLimits,
    consumed: BudgetAmounts,
    operation_reserved: BudgetAmounts,
    child_delegated_remaining: BudgetAmounts,
    available: BudgetAmounts,
    phase: BudgetAccountPhase,
}

impl BudgetSnapshot {
    pub(crate) const fn from_account(
        account: &BudgetAccount,
        available: BudgetAmounts,
    ) -> (result: Self)
        ensures account_snapshot_exact(*account, result, available),
    {
        Self {
            id: account.id,
            parent_id: account.parent_id,
            revision: account.revision,
            limits: account.limits,
            consumed: account.consumed,
            operation_reserved: account.operation_reserved,
            child_delegated_remaining: account.child_delegated_remaining,
            available,
            phase: account.phase,
        }
    }

    /// Returns the account identity.
    #[must_use]
    pub const fn id(self) -> BudgetId { self.id }
    /// Returns the direct parent, or `None` for the root.
    #[must_use]
    pub const fn parent_id(self) -> Option<BudgetId> { self.parent_id }
    /// Returns the exact authority revision.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple { self.revision }
    /// Returns the immutable ceiling.
    #[must_use]
    pub const fn limits(self) -> BudgetLimits { self.limits }
    /// Returns monotonic authoritative consumption.
    #[must_use]
    pub const fn consumed(self) -> BudgetAmounts { self.consumed }
    /// Returns capacity held by live direct reservations.
    #[must_use]
    pub const fn operation_reserved(self) -> BudgetAmounts { self.operation_reserved }
    /// Returns unconsumed capacity delegated to direct and indirect descendants.
    #[must_use]
    pub const fn child_delegated_remaining(self) -> BudgetAmounts {
        self.child_delegated_remaining
    }
    /// Returns exact currently uncommitted capacity.
    #[must_use]
    pub const fn available(self) -> BudgetAmounts { self.available }
    /// Returns the account lifecycle phase.
    #[must_use]
    pub const fn phase(self) -> BudgetAccountPhase { self.phase }
}

/// Checked immutable reservation projection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReservationSnapshot {
    request: crate::BudgetRequest,
    observed: BudgetAmounts,
    outstanding: BudgetAmounts,
    phase: ReservationPhase,
    activation_evidence: Option<Sha256Digest>,
    observation_evidence: Option<Sha256Digest>,
    final_evidence: Option<Sha256Digest>,
    final_reported: Option<BudgetAmounts>,
    finality: Option<UsageFinality>,
}

impl ReservationSnapshot {
    pub(crate) const fn from_record(
        record: &ReservationRecord,
        outstanding: BudgetAmounts,
    ) -> (result: Self)
        ensures reservation_snapshot_exact(*record, result, outstanding),
    {
        Self {
            request: record.request,
            observed: record.observed,
            outstanding,
            phase: record.phase,
            activation_evidence: record.activation_evidence,
            observation_evidence: record.observation_evidence,
            final_evidence: record.final_evidence,
            final_reported: record.final_reported,
            finality: record.finality,
        }
    }

    /// Returns the immutable begin request.
    #[must_use]
    pub const fn request(self) -> crate::BudgetRequest { self.request }
    /// Returns the accepted cumulative high-water observation.
    #[must_use]
    pub const fn observed(self) -> BudgetAmounts { self.observed }
    /// Returns the ceiling still reserved, or zero for a finalized tombstone.
    #[must_use]
    pub const fn outstanding(self) -> BudgetAmounts { self.outstanding }
    /// Returns the reservation lifecycle phase.
    #[must_use]
    pub const fn phase(self) -> ReservationPhase { self.phase }
    /// Returns exact activation evidence when activation was accepted.
    #[must_use]
    pub const fn activation_evidence(self) -> Option<Sha256Digest> {
        self.activation_evidence
    }
    /// Returns the evidence bound to the latest accepted cumulative high-water mark.
    #[must_use]
    pub const fn observation_evidence(self) -> Option<Sha256Digest> {
        self.observation_evidence
    }
    /// Returns exact final evidence when the reservation was finalized.
    #[must_use]
    pub const fn final_evidence(self) -> Option<Sha256Digest> { self.final_evidence }
    /// Returns the exact final cumulative report, including an above-ceiling raw report.
    #[must_use]
    pub const fn final_reported(self) -> Option<BudgetAmounts> { self.final_reported }
    /// Returns the finality attached to a terminal cumulative observation.
    #[must_use]
    pub const fn finality(self) -> Option<UsageFinality> { self.finality }
}

pub(crate) closed spec fn account_snapshot_exact(
    account: BudgetAccount,
    snapshot: BudgetSnapshot,
    available: BudgetAmounts,
) -> bool {
    crate::identity_model::budget_ids_equal(account.id, snapshot.id)
        && crate::identity_model::parents_equal(account.parent_id, snapshot.parent_id)
        && crate::identity_model::revisions_equal(account.revision, snapshot.revision)
        && account.limits.spec_amounts().spec_equal(snapshot.limits.spec_amounts())
        && account.consumed.spec_equal(snapshot.consumed)
        && account.operation_reserved.spec_equal(snapshot.operation_reserved)
        && account.child_delegated_remaining.spec_equal(snapshot.child_delegated_remaining)
        && available.spec_equal(snapshot.available)
        && account.phase == snapshot.phase
}

pub(crate) closed spec fn reservation_snapshot_exact(
    record: ReservationRecord,
    snapshot: ReservationSnapshot,
    outstanding: BudgetAmounts,
) -> bool {
    crate::refinement_model::requests_equal(record.request, snapshot.request)
        && record.observed.spec_equal(snapshot.observed)
        && outstanding.spec_equal(snapshot.outstanding)
        && record.phase == snapshot.phase
        && crate::invariant::optional_digests_equal(
            record.activation_evidence, snapshot.activation_evidence,
        )
        && crate::invariant::optional_digests_equal(
            record.observation_evidence, snapshot.observation_evidence,
        )
        && crate::invariant::optional_digests_equal(
            record.final_evidence, snapshot.final_evidence,
        )
        && crate::invariant::optional_amounts_equal(
            record.final_reported, snapshot.final_reported,
        )
        && record.finality == snapshot.finality
}

pub(crate) open spec fn outstanding_is_exact(
    record: ReservationRecord,
    outstanding: BudgetAmounts,
) -> bool {
    outstanding.spec_get(crate::BudgetDimension::ModelTokens)
        == crate::accounting_model::record_outstanding(
            record, crate::BudgetDimension::ModelTokens,
        )
        && outstanding.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
            == crate::accounting_model::record_outstanding(
                record, crate::BudgetDimension::ProviderCostMicrounits,
            )
        && outstanding.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
            == crate::accounting_model::record_outstanding(
                record, crate::BudgetDimension::ActiveEffectMilliseconds,
            )
        && outstanding.spec_get(crate::BudgetDimension::Attempts)
            == crate::accounting_model::record_outstanding(
                record, crate::BudgetDimension::Attempts,
            )
        && outstanding.spec_get(crate::BudgetDimension::Retries)
            == crate::accounting_model::record_outstanding(
                record, crate::BudgetDimension::Retries,
            )
}


} // verus!
