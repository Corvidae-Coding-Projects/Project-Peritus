//! Privately represented ledger state, checked snapshots, and transition receipts.

#![allow(
    clippy::match_like_matches_macro,
    reason = "The explicit phase match is shared with Verus, where macro expansion is outside the audited API subset"
)]

mod queries;
mod receipt;

pub use self::receipt::{BudgetOperation, BudgetReceipt, BudgetReceiptKind, BudgetTransition};

use crate::{BudgetAmounts, BudgetError, BudgetLimits, UsageFinality};
use peritus_types::{BudgetId, RevisionTuple, Sha256Digest};

#[doc = " Lifecycle of one budget account."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BudgetAccountPhase {
    #[doc = " New child allocations and operation begins are accepted."]
    Open,
    #[doc = " New work is denied while existing work may settle."]
    Draining,
    #[doc = " An over-ceiling observation permanently denies new work."]
    Faulted,
    #[doc = " The account is quiescent and its unused parent delegation was released."]
    Closed,
}
impl BudgetAccountPhase {
    pub(crate) const fn is_closed(self) -> bool {
        matches!(self, Self::Closed)
    }
}
#[doc = " Lifecycle of one immutable reservation tombstone."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReservationPhase {
    #[doc = " Capacity is held, but no effect has been observed active."]
    Held,
    #[doc = " The effect has been observed active."]
    Active,
    #[doc = " Exact evidence consumed all outstanding reserved capacity."]
    SettledExact,
    #[doc = " A final cumulative report consumed its high-water use and released the remainder."]
    SettledFinal,
    #[doc = " Exact evidence proved the held operation never activated."]
    CancelledHeld,
    #[doc = " Ambiguity conservatively consumed all outstanding reserved capacity."]
    SettledAmbiguous,
    #[doc = " Above-ceiling evidence consumed the held ceiling and faulted the lineage."]
    OverrunFaulted,
}
impl ReservationPhase {
    pub(crate) const fn is_live(self) -> bool {
        matches!(self, Self::Held | Self::Active)
    }
    pub(crate) const fn equals(self, other: Self) -> bool {
        match (self, other) {
            (Self::Held, Self::Held)
            | (Self::Active, Self::Active)
            | (Self::SettledExact, Self::SettledExact)
            | (Self::SettledFinal, Self::SettledFinal)
            | (Self::CancelledHeld, Self::CancelledHeld)
            | (Self::SettledAmbiguous, Self::SettledAmbiguous)
            | (Self::OverrunFaulted, Self::OverrunFaulted) => true,
            _ => false,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BudgetAccount {
    pub(crate) id: BudgetId,
    pub(crate) parent_id: Option<BudgetId>,
    pub(crate) revision: RevisionTuple,
    pub(crate) limits: BudgetLimits,
    pub(crate) consumed: BudgetAmounts,
    pub(crate) operation_reserved: BudgetAmounts,
    pub(crate) child_delegated_remaining: BudgetAmounts,
    pub(crate) phase: BudgetAccountPhase,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReservationRecord {
    pub(crate) request: crate::BudgetRequest,
    pub(crate) observed: BudgetAmounts,
    pub(crate) phase: ReservationPhase,
    pub(crate) activation_evidence: Option<Sha256Digest>,
    pub(crate) observation_evidence: Option<Sha256Digest>,
    pub(crate) final_evidence: Option<Sha256Digest>,
    pub(crate) final_reported: Option<BudgetAmounts>,
    pub(crate) finality: Option<UsageFinality>,
}
#[doc = " Complete root-owned budget tree and immutable reservation tombstones."]
#[derive(Debug, Eq, PartialEq)]
pub struct BudgetLedger {
    pub(crate) root_id: BudgetId,
    pub(crate) accounts: Vec<BudgetAccount>,
    pub(crate) reservations: Vec<ReservationRecord>,
}
impl BudgetLedger {
    pub(crate) fn duplicate(&self) -> Self {
        let mut accounts = Vec::new();
        let mut account_index = 0;
        while account_index < self.accounts.len() {
            accounts.push(self.accounts[account_index]);
            account_index += 1;
        }
        let mut reservations = Vec::new();
        let mut reservation_index = 0;
        while reservation_index < self.reservations.len() {
            reservations.push(self.reservations[reservation_index]);
            reservation_index += 1;
        }
        Self { root_id: self.root_id, accounts, reservations }
    }
    #[doc = " Creates a ledger containing one open immutable root account."]
    #[must_use]
    pub fn new_root(root_id: BudgetId, revision: RevisionTuple, limits: BudgetLimits) -> Self {
        Self {
            root_id,
            accounts: vec![BudgetAccount {
                id: root_id,
                parent_id: None,
                revision,
                limits,
                consumed: BudgetAmounts::zero(),
                operation_reserved: BudgetAmounts::zero(),
                child_delegated_remaining: BudgetAmounts::zero(),
                phase: BudgetAccountPhase::Open,
            }],
            reservations: Vec::new(),
        }
    }
    #[doc = " Returns the unique root identity."]
    #[must_use]
    pub const fn root_id(&self) -> BudgetId {
        self.root_id
    }
    #[doc = " Returns the number of accounts, including closed tombstones."]
    #[must_use]
    pub const fn account_count(&self) -> usize {
        self.accounts.len()
    }
    #[doc = " Returns the number of reservation tombstones."]
    #[must_use]
    pub const fn reservation_count(&self) -> usize {
        self.reservations.len()
    }
    #[doc = " Applies one pure logical command and returns a new value plus a non-authorizing receipt."]
    #[doc = ""]
    #[doc = " A successful result does not prove persistence and grants no permission to dispatch an"]
    #[doc = " effect. The caller must durably commit the exact transition through the state-owner slice."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed failure without changing `self`."]
    #[allow(clippy::large_types_passed_by_value)]
    pub fn transition(
        &self,
        command: crate::BudgetCommand,
    ) -> Result<BudgetTransition, BudgetError> {
        crate::transition::apply(self, command)
    }
}
