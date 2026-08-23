//! Independent five-dimensional reference ledger for persisted generated traces.

use peritus_budget::{
    BudgetAccountPhase, BudgetAmounts, BudgetRequest, ReservationPhase, UsageFinality,
};
use peritus_types::{BudgetId, RevisionTuple, Sha256Digest};

mod assertions;
pub(crate) mod driver;
mod errors;
mod receipts;
mod units;

pub use driver::{Runner, amount, attempt, execution, fresh_request};
pub use receipts::ReceiptModel;
use units::Units;

#[derive(Clone, Copy)]
pub struct TracePoint {
    pub seed: u64,
    pub case: usize,
    pub step: usize,
}

struct AccountModel {
    id: BudgetId,
    parent: Option<BudgetId>,
    revision: RevisionTuple,
    limit: Units,
    consumed: Units,
    reserved: Units,
    delegated: Units,
    phase: BudgetAccountPhase,
}

struct ReservationModel {
    request: BudgetRequest,
    observed: Units,
    phase: ReservationPhase,
    activation: Option<Sha256Digest>,
    observation: Option<Sha256Digest>,
    final_evidence: Option<Sha256Digest>,
    final_reported: Option<Units>,
    finality: Option<UsageFinality>,
}

pub struct ReferenceModel {
    accounts: Vec<AccountModel>,
    reservations: Vec<ReservationModel>,
}

impl ReferenceModel {
    pub fn new(root: BudgetId, revision: RevisionTuple, limit: BudgetAmounts) -> Self {
        Self {
            accounts: vec![AccountModel {
                id: root,
                parent: None,
                revision,
                limit: Units::from_amount(limit),
                consumed: Units::zero(),
                reserved: Units::zero(),
                delegated: Units::zero(),
                phase: BudgetAccountPhase::Open,
            }],
            reservations: Vec::new(),
        }
    }

    pub fn allocate(
        &mut self,
        child: BudgetId,
        parent: BudgetId,
        revision: RevisionTuple,
        limit: BudgetAmounts,
    ) -> ReceiptModel {
        let units = Units::from_amount(limit);
        self.account_mut(parent).delegated.add(units);
        self.accounts.push(AccountModel {
            id: child,
            parent: Some(parent),
            revision,
            limit: units,
            consumed: Units::zero(),
            reserved: Units::zero(),
            delegated: Units::zero(),
            phase: BudgetAccountPhase::Open,
        });
        ReceiptModel::allocation(child)
    }

    pub fn begin(&mut self, request: BudgetRequest) -> ReceiptModel {
        let consume = Units::from_amount(request.consume_now());
        let reserve = Units::from_amount(request.reserve());
        self.charge_lineage(request.budget_id(), consume);
        self.account_mut(request.budget_id()).reserved.add(reserve);
        self.reservations.push(ReservationModel {
            request,
            observed: Units::zero(),
            phase: if reserve.is_zero() {
                ReservationPhase::SettledExact
            } else {
                ReservationPhase::Held
            },
            activation: None,
            observation: None,
            final_evidence: None,
            final_reported: None,
            finality: None,
        });
        ReceiptModel::begin(request)
    }

    pub fn activate(&mut self, request: BudgetRequest, evidence: Sha256Digest) -> ReceiptModel {
        let record = self.reservation_mut(request);
        record.phase = ReservationPhase::Active;
        record.activation = Some(evidence);
        ReceiptModel::activation(request, evidence)
    }

    pub fn observe(
        &mut self,
        request: BudgetRequest,
        cumulative: BudgetAmounts,
        evidence: Sha256Digest,
        finality: UsageFinality,
    ) -> ReceiptModel {
        let cumulative = Units::from_amount(cumulative);
        let (budget_id, prior, ceiling) = {
            let record = self.reservation(request);
            (request.budget_id(), record.observed, Units::from_amount(request.reserve()))
        };
        let charged = cumulative.subtracted(prior);
        self.account_mut(budget_id).reserved.sub(charged);
        self.charge_lineage(budget_id, charged);
        let record = self.reservation_mut(request);
        record.observed = cumulative;
        record.observation = Some(evidence);
        let released = if finality == UsageFinality::Final {
            let released = ceiling.subtracted(cumulative);
            self.account_mut(budget_id).reserved.sub(released);
            let record = self.reservation_mut(request);
            record.phase = ReservationPhase::SettledFinal;
            record.final_evidence = Some(evidence);
            record.final_reported = Some(cumulative);
            record.finality = Some(UsageFinality::Final);
            released
        } else {
            Units::zero()
        };
        ReceiptModel::observation(
            request,
            peritus_budget::BudgetReceiptKind::Applied,
            charged.amount(),
            released.amount(),
            cumulative.amount(),
            evidence,
        )
    }

    pub fn overrun(
        &mut self,
        request: BudgetRequest,
        reported: BudgetAmounts,
        evidence: Sha256Digest,
        finality: UsageFinality,
    ) -> ReceiptModel {
        let budget_id = request.budget_id();
        let ceiling = Units::from_amount(request.reserve());
        let prior = self.reservation(request).observed;
        let charged = ceiling.subtracted(prior);
        self.account_mut(budget_id).reserved.sub(charged);
        self.charge_lineage(budget_id, charged);
        let record = self.reservation_mut(request);
        record.observed = ceiling;
        record.observation = Some(evidence);
        record.phase = ReservationPhase::OverrunFaulted;
        record.final_evidence = Some(evidence);
        record.final_reported = Some(Units::from_amount(reported));
        record.finality = Some(finality);
        self.fault_lineage(budget_id);
        ReceiptModel::observation(
            request,
            peritus_budget::BudgetReceiptKind::OverrunFaulted,
            charged.amount(),
            BudgetAmounts::zero(),
            reported,
            evidence,
        )
    }

    pub fn consume_remainder(
        &mut self,
        request: BudgetRequest,
        evidence: Sha256Digest,
        phase: ReservationPhase,
    ) -> ReceiptModel {
        let budget_id = request.budget_id();
        let ceiling = Units::from_amount(request.reserve());
        let prior = self.reservation(request).observed;
        let charged = ceiling.subtracted(prior);
        self.account_mut(budget_id).reserved.sub(charged);
        self.charge_lineage(budget_id, charged);
        let record = self.reservation_mut(request);
        record.observed = ceiling;
        record.phase = phase;
        record.final_evidence = Some(evidence);
        ReceiptModel::finalization(request, phase, charged.amount(), evidence)
    }

    pub fn cancel(&mut self, request: BudgetRequest, evidence: Sha256Digest) -> ReceiptModel {
        self.account_mut(request.budget_id()).reserved.sub(Units::from_amount(request.reserve()));
        let record = self.reservation_mut(request);
        record.phase = ReservationPhase::CancelledHeld;
        record.final_evidence = Some(evidence);
        ReceiptModel::cancellation(request, evidence)
    }

    pub fn seal(&mut self, budget_id: BudgetId) -> ReceiptModel {
        let account = self.account_mut(budget_id);
        if account.phase == BudgetAccountPhase::Open {
            account.phase = BudgetAccountPhase::Draining;
        }
        ReceiptModel::account(peritus_budget::BudgetOperation::Seal, budget_id, Units::zero())
    }

    pub fn close(&mut self, budget_id: BudgetId) -> ReceiptModel {
        let (parent, unused) = {
            let account = self.account(budget_id);
            (account.parent, account.limit.subtracted(account.consumed))
        };
        if let Some(parent) = parent {
            self.account_mut(parent).delegated.sub(unused);
        }
        self.account_mut(budget_id).phase = BudgetAccountPhase::Closed;
        ReceiptModel::account(peritus_budget::BudgetOperation::Close, budget_id, unused)
    }

    fn charge_lineage(&mut self, budget_id: BudgetId, amount: Units) {
        let mut current = budget_id;
        let mut delegated = false;
        loop {
            let account = self.account_mut(current);
            if delegated {
                account.delegated.sub(amount);
            }
            account.consumed.add(amount);
            match account.parent {
                Some(parent) => {
                    current = parent;
                    delegated = true;
                }
                None => break,
            }
        }
    }

    fn fault_lineage(&mut self, budget_id: BudgetId) {
        let mut current = Some(budget_id);
        while let Some(id) = current {
            let account = self.account_mut(id);
            account.phase = BudgetAccountPhase::Faulted;
            current = account.parent;
        }
    }

    fn account(&self, id: BudgetId) -> &AccountModel {
        self.accounts.iter().find(|account| account.id == id).expect("modeled account")
    }

    fn account_mut(&mut self, id: BudgetId) -> &mut AccountModel {
        self.accounts.iter_mut().find(|account| account.id == id).expect("modeled account")
    }

    fn reservation(&self, request: BudgetRequest) -> &ReservationModel {
        self.reservations
            .iter()
            .find(|record| record.request.reservation_id() == request.reservation_id())
            .expect("modeled reservation")
    }

    fn reservation_mut(&mut self, request: BudgetRequest) -> &mut ReservationModel {
        self.reservations
            .iter_mut()
            .find(|record| record.request.reservation_id() == request.reservation_id())
            .expect("modeled reservation")
    }
}
