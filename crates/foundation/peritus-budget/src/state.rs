//! Privately represented ledger state, checked snapshots, and transition receipts.

#![allow(
    clippy::match_like_matches_macro,
    reason = "The explicit phase match is shared with Verus, where macro expansion is outside the audited API subset"
)]

mod receipt;
mod queries;

#[cfg(verus_only)]
pub(crate) use self::receipt::optional_reservation_ids_equal;
pub use self::receipt::{
    BudgetOperation, BudgetReceipt, BudgetReceiptKind, BudgetTransition,
};

use crate::{BudgetAmounts, BudgetError, BudgetLimits, UsageFinality};
use peritus_types::{BudgetId, RevisionTuple, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Lifecycle of one budget account.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BudgetAccountPhase {
    /// New child allocations and operation begins are accepted.
    Open,
    /// New work is denied while existing work may settle.
    Draining,
    /// An over-ceiling observation permanently denies new work.
    Faulted,
    /// The account is quiescent and its unused parent delegation was released.
    Closed,
}

impl BudgetAccountPhase {
    pub(crate) open spec fn spec_is_closed(self) -> bool { self == Self::Closed }

    pub(crate) const fn is_closed(self) -> (result: bool)
        ensures result == self.spec_is_closed(),
    {
        matches!(self, Self::Closed)
    }
}

/// Lifecycle of one immutable reservation tombstone.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReservationPhase {
    /// Capacity is held, but no effect has been observed active.
    Held,
    /// The effect has been observed active.
    Active,
    /// Exact evidence consumed all outstanding reserved capacity.
    SettledExact,
    /// A final cumulative report consumed its high-water use and released the remainder.
    SettledFinal,
    /// Exact evidence proved the held operation never activated.
    CancelledHeld,
    /// Ambiguity conservatively consumed all outstanding reserved capacity.
    SettledAmbiguous,
    /// Above-ceiling evidence consumed the held ceiling and faulted the lineage.
    OverrunFaulted,
}

impl ReservationPhase {
    pub(crate) open spec fn spec_is_live(self) -> bool {
        self == Self::Held || self == Self::Active
    }

    pub(crate) const fn is_live(self) -> (result: bool)
        ensures result == self.spec_is_live(),
    {
        matches!(self, Self::Held | Self::Active)
    }

    pub(crate) const fn equals(self, other: Self) -> (result: bool)
        ensures result == (self == other),
    {
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

/// Complete root-owned budget tree and immutable reservation tombstones.
#[derive(Debug, Eq, PartialEq)]
pub struct BudgetLedger {
    pub(crate) root_id: BudgetId,
    pub(crate) accounts: Vec<BudgetAccount>,
    pub(crate) reservations: Vec<ReservationRecord>,
}

impl BudgetLedger {
    /// Mathematical root identity.
    pub closed spec fn spec_root_id(&self) -> BudgetId { self.root_id }

    /// Mathematical root revision for an initial ledger.
    pub closed spec fn spec_root_revision(&self) -> RevisionTuple { self.accounts[0].revision }

    /// Mathematical root limits for an initial ledger.
    pub closed spec fn spec_root_limits(&self) -> BudgetAmounts {
        self.accounts[0].limits.spec_amounts()
    }

    /// Mathematical predicate for the unique zero-accounting open-root constructor state.
    pub closed spec fn spec_is_initial(&self) -> bool {
        crate::model::ledger_well_formed(self)
            && self.accounts@.len() == 1
            && self.reservations@.len() == 0
            && crate::identity_model::budget_ids_equal(self.accounts[0].id, self.root_id)
            && self.accounts[0].parent_id.is_none()
            && self.accounts[0].consumed.spec_is_zero()
            && self.accounts[0].operation_reserved.spec_is_zero()
            && self.accounts[0].child_delegated_remaining.spec_is_zero()
            && self.accounts[0].phase == BudgetAccountPhase::Open
    }

    /// Opaque total refinement relation for one command result.
    ///
    /// Successful results name the unique admitted successor and exact receipt. Rejected results
    /// preserve this ledger and carry the exact branch-ordered typed failure. The body is closed so
    /// clients can rely on the contract without depending on private ledger representation.
    pub closed spec fn spec_transition_result(
        &self,
        command: crate::BudgetCommand,
        result: Result<BudgetTransition, BudgetError>,
    ) -> bool {
        match result {
            Ok(transition) => crate::reachability::budget_step(
                self,
                command,
                crate::reachability::BudgetStepOutcome::Accepted(
                    transition.spec_ledger(),
                    transition.spec_receipt(),
                ),
            ),
            Err(error) => crate::reachability::budget_step(
                self,
                command,
                crate::reachability::BudgetStepOutcome::Rejected(*self, error),
            ),
        }
    }

    pub(crate) proof fn accepted_result_is_exact(
        &self,
        command: crate::BudgetCommand,
        transition: BudgetTransition,
    )
        requires crate::reachability::budget_step(
            self,
            command,
            crate::reachability::BudgetStepOutcome::Accepted(
                transition.spec_ledger(),
                transition.spec_receipt(),
            ),
        ),
        ensures self.spec_transition_result(command, Ok(transition)),
    {
    }

    pub(crate) proof fn rejected_result_is_exact(
        &self,
        command: crate::BudgetCommand,
        error: BudgetError,
    )
        requires crate::reachability::budget_step(
            self,
            command,
            crate::reachability::BudgetStepOutcome::Rejected(*self, error),
        ),
        ensures self.spec_transition_result(command, Err(error)),
    {
    }

    pub(crate) fn duplicate(&self) -> (result: Self)
        ensures
            result.root_id == self.root_id,
            crate::identity_model::budget_ids_equal(result.root_id, self.root_id),
            result.accounts@ == self.accounts@,
            result.reservations@ == self.reservations@,
    {
        let mut accounts = Vec::new();
        let mut account_index = 0;
        while account_index < self.accounts.len()
            invariant
                0 <= account_index <= self.accounts.len(),
                accounts@ == self.accounts@.subrange(0, account_index as int),
            decreases self.accounts.len() - account_index,
        {
            accounts.push(self.accounts[account_index]);
            account_index += 1;
        }

        let mut reservations = Vec::new();
        let mut reservation_index = 0;
        while reservation_index < self.reservations.len()
            invariant
                0 <= reservation_index <= self.reservations.len(),
                reservations@ == self.reservations@.subrange(0, reservation_index as int),
            decreases self.reservations.len() - reservation_index,
        {
            reservations.push(self.reservations[reservation_index]);
            reservation_index += 1;
        }
        Self { root_id: self.root_id, accounts, reservations }
    }

    /// Creates a ledger containing one open immutable root account.
    #[must_use]
    pub fn new_root(
        root_id: BudgetId,
        revision: RevisionTuple,
        limits: BudgetLimits,
    ) -> (result: Self)
        ensures
            result.spec_is_initial(),
            result.spec_root_id() == root_id,
            result.spec_root_revision() == revision,
            result.spec_root_limits() == limits.spec_amounts(),
    {
        let result = Self {
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
        };
        #[cfg(verus_only)]
        let model_tokens_limit = limits.amounts().get(crate::BudgetDimension::ModelTokens).get();
        #[cfg(verus_only)]
        let provider_cost_limit = limits
            .amounts()
            .get(crate::BudgetDimension::ProviderCostMicrounits)
            .get();
        #[cfg(verus_only)]
        let active_effect_limit = limits
            .amounts()
            .get(crate::BudgetDimension::ActiveEffectMilliseconds)
            .get();
        #[cfg(verus_only)]
        let attempts_limit = limits.amounts().get(crate::BudgetDimension::Attempts).get();
        #[cfg(verus_only)]
        let retries_limit = limits.amounts().get(crate::BudgetDimension::Retries).get();
        proof {
            assert(model_tokens_limit >= 0);
            assert(provider_cost_limit >= 0);
            assert(active_effect_limit >= 0);
            assert(attempts_limit >= 0);
            assert(retries_limit >= 0);
            crate::reachability::single_root_is_well_formed(&result);
        }
        result
    }

    /// Returns the unique root identity.
    #[must_use]
    pub const fn root_id(&self) -> BudgetId { self.root_id }

    /// Returns the number of accounts, including closed tombstones.
    #[must_use]
    pub const fn account_count(&self) -> usize { self.accounts.len() }

    /// Returns the number of reservation tombstones.
    #[must_use]
    pub const fn reservation_count(&self) -> usize { self.reservations.len() }

    /// Applies one pure logical command and returns a new value plus a non-authorizing receipt.
    ///
    /// A successful result does not prove persistence and grants no permission to dispatch an
    /// effect. The caller must durably commit the exact transition through the state-owner slice.
    ///
    /// # Errors
    ///
    /// Returns a typed failure without changing `self`.
    #[allow(clippy::large_types_passed_by_value)]
    pub fn transition(
        &self,
        command: crate::BudgetCommand,
    ) -> (result: Result<BudgetTransition, BudgetError>)
        ensures self.spec_transition_result(command, result),
    {
        crate::transition::apply(self, command)
    }
}

} // verus!
