//! Closed command vocabulary for pure budget transitions.

use crate::{BudgetAmounts, BudgetLimits};
use peritus_types::{ActionId, BudgetId, BudgetReservationId, RevisionTuple, Sha256Digest};

mod evidence;
mod vocabulary;

pub use evidence::{
    Activation, AmbiguousFinalization, ReservationReference, UsageFinality, UsageObservation,
};
pub use vocabulary::BudgetCommand;

#[doc = " Allocates a new immutable child account from a direct parent."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChildBudgetRequest {
    child_id: BudgetId,
    parent_id: BudgetId,
    revision: RevisionTuple,
    limits: BudgetLimits,
}
impl ChildBudgetRequest {
    #[doc = " Creates an exact child-allocation request."]
    #[must_use]
    pub const fn new(
        child_id: BudgetId,
        parent_id: BudgetId,
        revision: RevisionTuple,
        limits: BudgetLimits,
    ) -> Self {
        Self { child_id, parent_id, revision, limits }
    }
    #[doc = " Returns the new child identity."]
    #[must_use]
    pub const fn child_id(self) -> BudgetId {
        self.child_id
    }
    pub(crate) const fn verified_child_id(self) -> BudgetId {
        self.child_id
    }
    #[doc = " Returns the direct parent identity."]
    #[must_use]
    pub const fn parent_id(self) -> BudgetId {
        self.parent_id
    }
    pub(crate) const fn verified_parent_id(self) -> BudgetId {
        self.parent_id
    }
    #[doc = " Returns the exact authority revision."]
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
    pub(crate) const fn verified_revision(self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the immutable child limits."]
    #[must_use]
    pub const fn limits(self) -> BudgetLimits {
        self.limits
    }
    pub(crate) const fn verified_limits(self) -> BudgetLimits {
        self.limits
    }
}
#[doc = " Atomically charges known use and reserves a ceiling for one operation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetRequest {
    reservation_id: BudgetReservationId,
    budget_id: BudgetId,
    revision: RevisionTuple,
    action_id: ActionId,
    action_digest: Sha256Digest,
    consume_now: BudgetAmounts,
    reserve: BudgetAmounts,
}
impl BudgetRequest {
    pub(crate) const fn matches(self, other: Self) -> bool {
        crate::identity_model::reservation_id_equal(
            self.verified_reservation_id(),
            other.verified_reservation_id(),
        ) && crate::identity_model::budget_id_equal(self.budget_id(), other.budget_id())
            && crate::identity_model::revision_equal(
                self.verified_revision(),
                other.verified_revision(),
            )
            && crate::identity_model::action_id_equal(
                self.verified_action_id(),
                other.verified_action_id(),
            )
            && crate::identity_model::digest_equal(
                self.verified_action_digest(),
                other.verified_action_digest(),
            )
            && self.verified_consume_now().equals(other.verified_consume_now())
            && self.reserve().equals(other.reserve())
    }
    #[doc = " Creates an exact begin request."]
    #[must_use]
    pub const fn new(
        reservation_id: BudgetReservationId,
        budget_id: BudgetId,
        revision: RevisionTuple,
        action_id: ActionId,
        action_digest: Sha256Digest,
        consume_now: BudgetAmounts,
        reserve: BudgetAmounts,
    ) -> Self {
        Self { reservation_id, budget_id, revision, action_id, action_digest, consume_now, reserve }
    }
    #[doc = " Returns the idempotency identity for this operation lineage."]
    #[must_use]
    pub const fn reservation_id(self) -> BudgetReservationId {
        self.reservation_id
    }
    pub(crate) const fn verified_reservation_id(self) -> BudgetReservationId {
        self.reservation_id
    }
    #[doc = " Returns the charged account identity."]
    #[must_use]
    pub const fn budget_id(self) -> BudgetId {
        self.budget_id
    }
    #[doc = " Returns the exact authority revision."]
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
    pub(crate) const fn verified_revision(self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the action identity."]
    #[must_use]
    pub const fn action_id(self) -> ActionId {
        self.action_id
    }
    pub(crate) const fn verified_action_id(self) -> ActionId {
        self.action_id
    }
    #[doc = " Returns the exact action-content digest."]
    #[must_use]
    pub const fn action_digest(self) -> Sha256Digest {
        self.action_digest
    }
    pub(crate) const fn verified_action_digest(self) -> Sha256Digest {
        self.action_digest
    }
    #[doc = " Returns usage charged immediately by begin."]
    #[must_use]
    pub const fn consume_now(self) -> BudgetAmounts {
        self.consume_now
    }
    pub(crate) const fn verified_consume_now(self) -> BudgetAmounts {
        self.consume_now
    }
    #[doc = " Returns the operation ceiling held after begin."]
    #[must_use]
    pub const fn reserve(self) -> BudgetAmounts {
        self.reserve
    }
}
