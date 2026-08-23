//! Independent exact receipt model for accepted trace steps.

use super::{ReferenceModel, TracePoint, Units};
use peritus_budget::{
    BudgetAmounts, BudgetCommand, BudgetOperation, BudgetReceipt, BudgetReceiptKind, BudgetRequest,
    ReservationPhase,
};
use peritus_types::{BudgetId, BudgetReservationId, Sha256Digest};

#[derive(Clone, Copy)]
pub struct ReceiptModel {
    operation: BudgetOperation,
    kind: BudgetReceiptKind,
    budget_id: BudgetId,
    reservation_id: Option<BudgetReservationId>,
    charged: BudgetAmounts,
    released: BudgetAmounts,
    reported: Option<BudgetAmounts>,
    evidence: Option<Sha256Digest>,
}

impl ReceiptModel {
    pub(super) const fn allocation(child: BudgetId) -> Self {
        Self::empty(BudgetOperation::AllocateChild, BudgetReceiptKind::Applied, child, None, None)
    }

    pub(super) const fn begin(request: BudgetRequest) -> Self {
        Self {
            operation: BudgetOperation::Begin,
            kind: BudgetReceiptKind::Applied,
            budget_id: request.budget_id(),
            reservation_id: Some(request.reservation_id()),
            charged: request.consume_now(),
            released: BudgetAmounts::zero(),
            reported: None,
            evidence: None,
        }
    }

    pub(super) const fn activation(request: BudgetRequest, evidence: Sha256Digest) -> Self {
        Self::empty(
            BudgetOperation::Activate,
            BudgetReceiptKind::Applied,
            request.budget_id(),
            Some(request.reservation_id()),
            Some(evidence),
        )
    }

    pub(super) const fn observation(
        request: BudgetRequest,
        kind: BudgetReceiptKind,
        charged: BudgetAmounts,
        released: BudgetAmounts,
        reported: BudgetAmounts,
        evidence: Sha256Digest,
    ) -> Self {
        Self {
            operation: BudgetOperation::ObserveUsage,
            kind,
            budget_id: request.budget_id(),
            reservation_id: Some(request.reservation_id()),
            charged,
            released,
            reported: Some(reported),
            evidence: Some(evidence),
        }
    }

    pub(super) const fn finalization(
        request: BudgetRequest,
        phase: ReservationPhase,
        charged: BudgetAmounts,
        evidence: Sha256Digest,
    ) -> Self {
        let operation = match phase {
            ReservationPhase::SettledExact => BudgetOperation::SettleExact,
            ReservationPhase::SettledAmbiguous => BudgetOperation::FinalizeAmbiguous,
            _ => panic!("reference finalization phase must consume the outstanding ceiling"),
        };
        Self {
            operation,
            kind: BudgetReceiptKind::Applied,
            budget_id: request.budget_id(),
            reservation_id: Some(request.reservation_id()),
            charged,
            released: BudgetAmounts::zero(),
            reported: None,
            evidence: Some(evidence),
        }
    }

    pub(super) const fn cancellation(request: BudgetRequest, evidence: Sha256Digest) -> Self {
        Self {
            operation: BudgetOperation::CancelHeld,
            kind: BudgetReceiptKind::Applied,
            budget_id: request.budget_id(),
            reservation_id: Some(request.reservation_id()),
            charged: BudgetAmounts::zero(),
            released: request.reserve(),
            reported: None,
            evidence: Some(evidence),
        }
    }

    pub(super) const fn account(
        operation: BudgetOperation,
        budget_id: BudgetId,
        released: Units,
    ) -> Self {
        Self {
            operation,
            kind: BudgetReceiptKind::Applied,
            budget_id,
            reservation_id: None,
            charged: BudgetAmounts::zero(),
            released: released.amount(),
            reported: None,
            evidence: None,
        }
    }

    const fn empty(
        operation: BudgetOperation,
        kind: BudgetReceiptKind,
        budget_id: BudgetId,
        reservation_id: Option<BudgetReservationId>,
        evidence: Option<Sha256Digest>,
    ) -> Self {
        Self {
            operation,
            kind,
            budget_id,
            reservation_id,
            charged: BudgetAmounts::zero(),
            released: BudgetAmounts::zero(),
            reported: None,
            evidence,
        }
    }

    pub fn assert_exact(self, actual: BudgetReceipt, point: &TracePoint) {
        assert_eq!(actual.operation(), self.operation, "{}", point.label());
        assert_eq!(actual.kind(), self.kind, "{}", point.label());
        assert_eq!(actual.budget_id(), self.budget_id, "{}", point.label());
        assert_eq!(actual.reservation_id(), self.reservation_id, "{}", point.label());
        assert_eq!(actual.charged(), self.charged, "{}", point.label());
        assert_eq!(actual.released(), self.released, "{}", point.label());
        assert_eq!(actual.reported(), self.reported, "{}", point.label());
        assert_eq!(actual.evidence_digest(), self.evidence, "{}", point.label());
    }
}

impl ReferenceModel {
    pub fn replay(&self, command: BudgetCommand, kind: BudgetReceiptKind) -> ReceiptModel {
        let mut expected = match command {
            BudgetCommand::AllocateChild(request) => ReceiptModel::empty(
                BudgetOperation::AllocateChild,
                kind,
                request.child_id(),
                None,
                None,
            ),
            BudgetCommand::Begin(request) => ReceiptModel::empty(
                BudgetOperation::Begin,
                kind,
                request.budget_id(),
                Some(request.reservation_id()),
                None,
            ),
            BudgetCommand::Activate(activation) => {
                let request = self.request(activation.reservation_id());
                ReceiptModel::empty(
                    BudgetOperation::Activate,
                    kind,
                    request.budget_id(),
                    Some(request.reservation_id()),
                    Some(activation.evidence_digest()),
                )
            }
            BudgetCommand::ObserveUsage(observation) => {
                let request = self.request(observation.reservation_id());
                ReceiptModel::observation(
                    request,
                    kind,
                    BudgetAmounts::zero(),
                    BudgetAmounts::zero(),
                    observation.cumulative(),
                    observation.evidence_digest(),
                )
            }
            BudgetCommand::SettleExact(reference) => {
                self.reference_replay(BudgetOperation::SettleExact, kind, reference)
            }
            BudgetCommand::CancelHeld(reference) => {
                self.reference_replay(BudgetOperation::CancelHeld, kind, reference)
            }
            BudgetCommand::FinalizeAmbiguous(ambiguous) => self.reference_replay(
                BudgetOperation::FinalizeAmbiguous,
                kind,
                ambiguous.reference(),
            ),
            BudgetCommand::Seal(budget_id) => {
                ReceiptModel::empty(BudgetOperation::Seal, kind, budget_id, None, None)
            }
            BudgetCommand::Close(budget_id) => {
                ReceiptModel::empty(BudgetOperation::Close, kind, budget_id, None, None)
            }
        };
        expected.kind = kind;
        expected
    }

    fn reference_replay(
        &self,
        operation: BudgetOperation,
        kind: BudgetReceiptKind,
        reference: peritus_budget::ReservationReference,
    ) -> ReceiptModel {
        let request = self.request(reference.reservation_id());
        ReceiptModel::empty(
            operation,
            kind,
            request.budget_id(),
            Some(request.reservation_id()),
            Some(reference.evidence_digest()),
        )
    }

    fn request(&self, reservation_id: BudgetReservationId) -> BudgetRequest {
        self.reservations
            .iter()
            .find(|record| record.request.reservation_id() == reservation_id)
            .expect("modeled replay reservation")
            .request
    }
}
