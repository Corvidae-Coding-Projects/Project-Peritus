//! Correlated activation, usage, settlement, and ambiguity evidence vocabulary.

use crate::BudgetAmounts;
use peritus_types::{ActionId, BudgetReservationId, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Binds a held reservation to evidence that execution became active.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Activation {
    reservation_id: BudgetReservationId,
    action_id: ActionId,
    action_digest: Sha256Digest,
    evidence_digest: Sha256Digest,
}

impl Activation {
    pub(crate) closed spec fn spec_reservation_id(&self) -> BudgetReservationId { self.reservation_id }
    pub(crate) closed spec fn spec_action_id(&self) -> ActionId { self.action_id }
    pub(crate) closed spec fn spec_action_digest(&self) -> Sha256Digest { self.action_digest }
    pub(crate) closed spec fn spec_evidence_digest(&self) -> Sha256Digest { self.evidence_digest }

    /// Creates an activation observation.
    #[must_use]
    pub const fn new(
        reservation_id: BudgetReservationId,
        action_id: ActionId,
        action_digest: Sha256Digest,
        evidence_digest: Sha256Digest,
    ) -> Self {
        Self { reservation_id, action_id, action_digest, evidence_digest }
    }

    /// Returns the reservation identity.
    #[must_use]
    pub const fn reservation_id(self) -> BudgetReservationId { self.reservation_id }
    pub(crate) const fn verified_reservation_id(self) -> (result: BudgetReservationId)
        ensures result == self.spec_reservation_id(),
    { self.reservation_id }

    /// Returns the action identity.
    #[must_use]
    pub const fn action_id(self) -> ActionId { self.action_id }
    pub(crate) const fn verified_action_id(self) -> (result: ActionId)
        ensures result == self.spec_action_id(),
    { self.action_id }

    /// Returns the exact action-content digest.
    #[must_use]
    pub const fn action_digest(self) -> Sha256Digest { self.action_digest }
    pub(crate) const fn verified_action_digest(self) -> (result: Sha256Digest)
        ensures result == self.spec_action_digest(),
    { self.action_digest }

    /// Returns the exact activation-evidence digest.
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest { self.evidence_digest }
    pub(crate) const fn verified_evidence_digest(self) -> (result: Sha256Digest)
        ensures result == self.spec_evidence_digest(),
    { self.evidence_digest }
}

/// Whether a cumulative observation keeps the reservation open or settles it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UsageFinality {
    /// More cumulative observations may follow.
    Interim,
    /// This is the definitive final cumulative observation.
    Final,
}

/// A cumulative high-water usage observation from an active operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UsageObservation {
    reservation_id: BudgetReservationId,
    action_id: ActionId,
    action_digest: Sha256Digest,
    evidence_digest: Sha256Digest,
    cumulative: BudgetAmounts,
    finality: UsageFinality,
}

impl UsageObservation {
    pub(crate) closed spec fn spec_reservation_id(&self) -> BudgetReservationId { self.reservation_id }
    pub(crate) closed spec fn spec_action_id(&self) -> ActionId { self.action_id }
    pub(crate) closed spec fn spec_action_digest(&self) -> Sha256Digest { self.action_digest }
    pub(crate) closed spec fn spec_evidence_digest(&self) -> Sha256Digest { self.evidence_digest }
    pub(crate) closed spec fn spec_cumulative(&self) -> BudgetAmounts { self.cumulative }
    pub(crate) closed spec fn spec_finality(&self) -> UsageFinality { self.finality }

    /// Creates an exact cumulative usage observation.
    #[must_use]
    pub const fn new(
        reservation_id: BudgetReservationId,
        action_id: ActionId,
        action_digest: Sha256Digest,
        evidence_digest: Sha256Digest,
        cumulative: BudgetAmounts,
        finality: UsageFinality,
    ) -> Self {
        Self { reservation_id, action_id, action_digest, evidence_digest, cumulative, finality }
    }

    /// Returns the reservation identity.
    #[must_use]
    pub const fn reservation_id(self) -> BudgetReservationId { self.reservation_id }
    pub(crate) const fn verified_reservation_id(self) -> (result: BudgetReservationId)
        ensures result == self.spec_reservation_id(),
    { self.reservation_id }
    /// Returns the action identity.
    #[must_use]
    pub const fn action_id(self) -> ActionId { self.action_id }
    pub(crate) const fn verified_action_id(self) -> (result: ActionId)
        ensures result == self.spec_action_id(),
    { self.action_id }
    /// Returns the exact action-content digest.
    #[must_use]
    pub const fn action_digest(self) -> Sha256Digest { self.action_digest }
    pub(crate) const fn verified_action_digest(self) -> (result: Sha256Digest)
        ensures result == self.spec_action_digest(),
    { self.action_digest }
    /// Returns the exact observation-evidence digest.
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest { self.evidence_digest }
    pub(crate) const fn verified_evidence_digest(self) -> (result: Sha256Digest)
        ensures result == self.spec_evidence_digest(),
    { self.evidence_digest }
    /// Returns the cumulative usage since activation, never an incremental delta.
    #[must_use]
    pub const fn cumulative(self) -> BudgetAmounts { self.cumulative }
    pub(crate) const fn verified_cumulative(self) -> (result: BudgetAmounts)
        ensures result == self.spec_cumulative(),
    { self.cumulative }
    /// Returns whether this observation is final.
    #[must_use]
    pub const fn finality(self) -> UsageFinality { self.finality }
    pub(crate) const fn verified_finality(self) -> (result: UsageFinality)
        ensures result == self.spec_finality(),
    { self.finality }
}

/// Exact correlation fields for settlement or held cancellation.
///
/// This value is freely constructible and therefore proves only identity and digest equality with
/// a reservation tombstone. In particular, it does not prove that an external operation never
/// activated and is not authority to commit [`crate::BudgetCommand::CancelHeld`]. Under
/// `REF-C0-B1-COMMIT-ONCE`, C0 must independently match the committed begin lineage and its own
/// non-forgeable authoritative target or journal observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReservationReference {
    reservation_id: BudgetReservationId,
    action_id: ActionId,
    action_digest: Sha256Digest,
    evidence_digest: Sha256Digest,
}

impl ReservationReference {
    pub(crate) closed spec fn spec_reservation_id(&self) -> BudgetReservationId { self.reservation_id }
    pub(crate) closed spec fn spec_action_id(&self) -> ActionId { self.action_id }
    pub(crate) closed spec fn spec_action_digest(&self) -> Sha256Digest { self.action_digest }
    pub(crate) closed spec fn spec_evidence_digest(&self) -> Sha256Digest { self.evidence_digest }

    /// Creates an exact correlated reservation reference.
    ///
    /// Caller-provided bytes remain an unprivileged claim; construction does not attest their
    /// provenance or any external negative fact.
    #[must_use]
    pub const fn new(
        reservation_id: BudgetReservationId,
        action_id: ActionId,
        action_digest: Sha256Digest,
        evidence_digest: Sha256Digest,
    ) -> Self {
        Self { reservation_id, action_id, action_digest, evidence_digest }
    }

    /// Returns the reservation identity.
    #[must_use]
    pub const fn reservation_id(self) -> BudgetReservationId { self.reservation_id }
    pub(crate) const fn verified_reservation_id(self) -> (result: BudgetReservationId)
        ensures result == self.spec_reservation_id(),
    { self.reservation_id }

    /// Returns the action identity.
    #[must_use]
    pub const fn action_id(self) -> ActionId { self.action_id }
    pub(crate) const fn verified_action_id(self) -> (result: ActionId)
        ensures result == self.spec_action_id(),
    { self.action_id }

    /// Returns the exact action-content digest.
    #[must_use]
    pub const fn action_digest(self) -> Sha256Digest { self.action_digest }
    pub(crate) const fn verified_action_digest(self) -> (result: Sha256Digest)
        ensures result == self.spec_action_digest(),
    { self.action_digest }

    /// Returns the exact settlement-evidence digest.
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest { self.evidence_digest }
    pub(crate) const fn verified_evidence_digest(self) -> (result: Sha256Digest)
        ensures result == self.spec_evidence_digest(),
    { self.evidence_digest }
}

/// Correlated evidence that an active outcome is indeterminate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AmbiguousFinalization {
    reference: ReservationReference,
}

impl AmbiguousFinalization {
    pub(crate) closed spec fn spec_reference(&self) -> ReservationReference { self.reference }
    /// Creates a conservative finalization for the exact reservation binding.
    #[must_use]
    pub const fn new(reference: ReservationReference) -> Self { Self { reference } }
    /// Returns the exact correlated reference.
    #[must_use]
    pub const fn reference(self) -> ReservationReference { self.reference }
    pub(crate) const fn verified_reference(self) -> (result: ReservationReference)
        ensures
            result == self.spec_reference(),
            crate::identity_model::reservation_ids_equal(
                result.spec_reservation_id(),
                self.spec_reference().spec_reservation_id(),
            ),
            crate::identity_model::action_ids_equal(
                result.spec_action_id(),
                self.spec_reference().spec_action_id(),
            ),
            crate::identity_model::digests_equal(
                result.spec_action_digest(),
                self.spec_reference().spec_action_digest(),
            ),
            crate::identity_model::digests_equal(
                result.spec_evidence_digest(),
                self.spec_reference().spec_evidence_digest(),
            ),
    { self.reference }
}

} // verus!
