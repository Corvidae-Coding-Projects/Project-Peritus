// Each integration-test target uses a different subset of this deterministic fixture vocabulary.
#![allow(dead_code)]

use peritus_budget::{
    Activation, BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits, BudgetRequest,
    ReservationReference, UsageFinality, UsageObservation,
};
use peritus_test_support::DeterministicIdSource;
use peritus_types::{
    AcceptanceSpecId, ActionId, BudgetId, BudgetReservationId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

pub struct Fixture {
    ids: DeterministicIdSource,
    pub revision: RevisionTuple,
    pub root_id: BudgetId,
    pub action_id: ActionId,
    pub action_digest: Sha256Digest,
}

impl Fixture {
    pub fn new() -> Self {
        let mut ids = DeterministicIdSource::new(*b"budget01");
        let revision = RevisionTuple::new(
            ids.next(AcceptanceSpecId::new).expect("acceptance id"),
            ids.next(HarnessId::new).expect("harness id"),
            ids.next(WorkspaceId::new).expect("workspace id"),
            Generation::first(),
            RevisionNumber::first(),
            ids.next(PolicyId::new).expect("policy id"),
            ids.next(ProviderProfileId::new).expect("provider profile id"),
        );
        let root_id = ids.next(BudgetId::new).expect("root id");
        let action_id = ids.next(ActionId::new).expect("action id");
        Self { ids, revision, root_id, action_id, action_digest: digest(1) }
    }

    pub fn budget_id(&mut self) -> BudgetId {
        self.ids.next(BudgetId::new).expect("budget id")
    }

    pub fn reservation_id(&mut self) -> BudgetReservationId {
        self.ids.next(BudgetReservationId::new).expect("reservation id")
    }

    pub fn action_id(&mut self) -> ActionId {
        self.ids.next(ActionId::new).expect("action id")
    }

    pub fn ledger(&self, limits: BudgetAmounts) -> BudgetLedger {
        BudgetLedger::new_root(self.root_id, self.revision, BudgetLimits::new(limits))
    }

    pub fn request(
        &mut self,
        budget_id: BudgetId,
        consume_now: BudgetAmounts,
        reserve: BudgetAmounts,
    ) -> BudgetRequest {
        BudgetRequest::new(
            self.reservation_id(),
            budget_id,
            self.revision,
            self.action_id,
            self.action_digest,
            consume_now,
            reserve,
        )
    }
}

pub const fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::new([byte; 32])
}

#[allow(clippy::large_types_passed_by_value)]
pub fn accepted(ledger: &BudgetLedger, command: BudgetCommand) -> BudgetLedger {
    ledger.transition(command).expect("accepted transition").into_ledger()
}

pub const fn activate(request: BudgetRequest, evidence: u8) -> BudgetCommand {
    BudgetCommand::Activate(Activation::new(
        request.reservation_id(),
        request.action_id(),
        request.action_digest(),
        digest(evidence),
    ))
}

pub const fn observe(
    request: BudgetRequest,
    evidence: u8,
    cumulative: BudgetAmounts,
    finality: UsageFinality,
) -> BudgetCommand {
    BudgetCommand::ObserveUsage(UsageObservation::new(
        request.reservation_id(),
        request.action_id(),
        request.action_digest(),
        digest(evidence),
        cumulative,
        finality,
    ))
}

pub const fn reference(request: BudgetRequest, evidence: u8) -> ReservationReference {
    ReservationReference::new(
        request.reservation_id(),
        request.action_id(),
        request.action_digest(),
        digest(evidence),
    )
}
