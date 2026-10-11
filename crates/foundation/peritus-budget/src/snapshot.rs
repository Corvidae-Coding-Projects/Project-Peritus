//! Checked read-only projections of private ledger records.

use crate::state::{BudgetAccount, ReservationRecord};
use crate::{BudgetAccountPhase, BudgetAmounts, BudgetLimits, ReservationPhase, UsageFinality};
use peritus_types::{BudgetId, RevisionTuple, Sha256Digest};

#[doc = " Checked immutable account projection."]
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
    pub(crate) const fn from_account(account: &BudgetAccount, available: BudgetAmounts) -> Self {
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
    #[doc = " Returns the account identity."]
    #[must_use]
    pub const fn id(self) -> BudgetId {
        self.id
    }
    #[doc = " Returns the direct parent, or `None` for the root."]
    #[must_use]
    pub const fn parent_id(self) -> Option<BudgetId> {
        self.parent_id
    }
    #[doc = " Returns the exact authority revision."]
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the immutable ceiling."]
    #[must_use]
    pub const fn limits(self) -> BudgetLimits {
        self.limits
    }
    #[doc = " Returns monotonic authoritative consumption."]
    #[must_use]
    pub const fn consumed(self) -> BudgetAmounts {
        self.consumed
    }
    #[doc = " Returns capacity held by live direct reservations."]
    #[must_use]
    pub const fn operation_reserved(self) -> BudgetAmounts {
        self.operation_reserved
    }
    #[doc = " Returns unconsumed capacity delegated to direct and indirect descendants."]
    #[must_use]
    pub const fn child_delegated_remaining(self) -> BudgetAmounts {
        self.child_delegated_remaining
    }
    #[doc = " Returns exact currently uncommitted capacity."]
    #[must_use]
    pub const fn available(self) -> BudgetAmounts {
        self.available
    }
    #[doc = " Returns the account lifecycle phase."]
    #[must_use]
    pub const fn phase(self) -> BudgetAccountPhase {
        self.phase
    }
}
#[doc = " Checked immutable reservation projection."]
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
    ) -> Self {
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
    #[doc = " Returns the immutable begin request."]
    #[must_use]
    pub const fn request(self) -> crate::BudgetRequest {
        self.request
    }
    #[doc = " Returns the accepted cumulative high-water observation."]
    #[must_use]
    pub const fn observed(self) -> BudgetAmounts {
        self.observed
    }
    #[doc = " Returns the ceiling still reserved, or zero for a finalized tombstone."]
    #[must_use]
    pub const fn outstanding(self) -> BudgetAmounts {
        self.outstanding
    }
    #[doc = " Returns the reservation lifecycle phase."]
    #[must_use]
    pub const fn phase(self) -> ReservationPhase {
        self.phase
    }
    #[doc = " Returns exact activation evidence when activation was accepted."]
    #[must_use]
    pub const fn activation_evidence(self) -> Option<Sha256Digest> {
        self.activation_evidence
    }
    #[doc = " Returns the evidence bound to the latest accepted cumulative high-water mark."]
    #[must_use]
    pub const fn observation_evidence(self) -> Option<Sha256Digest> {
        self.observation_evidence
    }
    #[doc = " Returns exact final evidence when the reservation was finalized."]
    #[must_use]
    pub const fn final_evidence(self) -> Option<Sha256Digest> {
        self.final_evidence
    }
    #[doc = " Returns the exact final cumulative report, including an above-ceiling raw report."]
    #[must_use]
    pub const fn final_reported(self) -> Option<BudgetAmounts> {
        self.final_reported
    }
    #[doc = " Returns the finality attached to a terminal cumulative observation."]
    #[must_use]
    pub const fn finality(self) -> Option<UsageFinality> {
        self.finality
    }
}
