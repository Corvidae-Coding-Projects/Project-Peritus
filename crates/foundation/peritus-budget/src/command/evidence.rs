//! Correlated activation, usage, settlement, and ambiguity evidence vocabulary.

use crate::BudgetAmounts;
use peritus_types::{ActionId, BudgetReservationId, Sha256Digest};

#[doc = " Binds a held reservation to evidence that execution became active."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Activation {
    reservation_id: BudgetReservationId,
    action_id: ActionId,
    action_digest: Sha256Digest,
    evidence_digest: Sha256Digest,
}
impl Activation {
    #[doc = " Creates an activation observation."]
    #[must_use]
    pub const fn new(
        reservation_id: BudgetReservationId,
        action_id: ActionId,
        action_digest: Sha256Digest,
        evidence_digest: Sha256Digest,
    ) -> Self {
        Self { reservation_id, action_id, action_digest, evidence_digest }
    }
    #[doc = " Returns the reservation identity."]
    #[must_use]
    pub const fn reservation_id(self) -> BudgetReservationId {
        self.reservation_id
    }
    pub(crate) const fn verified_reservation_id(self) -> BudgetReservationId {
        self.reservation_id
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
    #[doc = " Returns the exact activation-evidence digest."]
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
    pub(crate) const fn verified_evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
}
#[doc = " Whether a cumulative observation keeps the reservation open or settles it."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UsageFinality {
    #[doc = " More cumulative observations may follow."]
    Interim,
    #[doc = " This is the definitive final cumulative observation."]
    Final,
}
#[doc = " A cumulative high-water usage observation from an active operation."]
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
    #[doc = " Creates an exact cumulative usage observation."]
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
    #[doc = " Returns the reservation identity."]
    #[must_use]
    pub const fn reservation_id(self) -> BudgetReservationId {
        self.reservation_id
    }
    pub(crate) const fn verified_reservation_id(self) -> BudgetReservationId {
        self.reservation_id
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
    #[doc = " Returns the exact observation-evidence digest."]
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
    pub(crate) const fn verified_evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
    #[doc = " Returns the cumulative usage since activation, never an incremental delta."]
    #[must_use]
    pub const fn cumulative(self) -> BudgetAmounts {
        self.cumulative
    }
    pub(crate) const fn verified_cumulative(self) -> BudgetAmounts {
        self.cumulative
    }
    #[doc = " Returns whether this observation is final."]
    #[must_use]
    pub const fn finality(self) -> UsageFinality {
        self.finality
    }
    pub(crate) const fn verified_finality(self) -> UsageFinality {
        self.finality
    }
}
#[doc = " Exact correlation fields for settlement or held cancellation."]
#[doc = ""]
#[doc = " This value is freely constructible and therefore proves only identity and digest equality with"]
#[doc = " a reservation tombstone. In particular, it does not prove that an external operation never"]
#[doc = " activated and is not authority to commit [`crate::BudgetCommand::CancelHeld`]. Under"]
#[doc = " `REF-C0-B1-COMMIT-ONCE`, C0 must independently match the committed begin lineage and its own"]
#[doc = " non-forgeable authoritative target or journal observation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReservationReference {
    reservation_id: BudgetReservationId,
    action_id: ActionId,
    action_digest: Sha256Digest,
    evidence_digest: Sha256Digest,
}
impl ReservationReference {
    #[doc = " Creates an exact correlated reservation reference."]
    #[doc = ""]
    #[doc = " Caller-provided bytes remain an unprivileged claim; construction does not attest their"]
    #[doc = " provenance or any external negative fact."]
    #[must_use]
    pub const fn new(
        reservation_id: BudgetReservationId,
        action_id: ActionId,
        action_digest: Sha256Digest,
        evidence_digest: Sha256Digest,
    ) -> Self {
        Self { reservation_id, action_id, action_digest, evidence_digest }
    }
    #[doc = " Returns the reservation identity."]
    #[must_use]
    pub const fn reservation_id(self) -> BudgetReservationId {
        self.reservation_id
    }
    pub(crate) const fn verified_reservation_id(self) -> BudgetReservationId {
        self.reservation_id
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
    #[doc = " Returns the exact settlement-evidence digest."]
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
    pub(crate) const fn verified_evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
}
#[doc = " Correlated evidence that an active outcome is indeterminate."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AmbiguousFinalization {
    reference: ReservationReference,
}
impl AmbiguousFinalization {
    #[doc = " Creates a conservative finalization for the exact reservation binding."]
    #[must_use]
    pub const fn new(reference: ReservationReference) -> Self {
        Self { reference }
    }
    #[doc = " Returns the exact correlated reference."]
    #[must_use]
    pub const fn reference(self) -> ReservationReference {
        self.reference
    }
    pub(crate) const fn verified_reference(self) -> ReservationReference {
        self.reference
    }
}
