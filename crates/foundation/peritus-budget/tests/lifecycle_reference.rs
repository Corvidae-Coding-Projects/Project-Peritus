//! Persisted generated lifecycle traces against an independent hierarchical ledger.

mod reference_support;
mod support;

use peritus_budget::{
    AmbiguousFinalization, BudgetAmounts, BudgetCommand, BudgetErrorKind, BudgetLedger,
    BudgetLimits, BudgetReceiptKind, BudgetRequest, ChildBudgetRequest, ReservationPhase,
    UsageFinality,
};
use reference_support::{
    Generator, ReceiptModel, ReferenceModel, Runner, amount, attempt, execution, fresh_request,
};
use support::{Fixture, activate, digest, observe, reference};

const TRACE_SEEDS: [u64; 4] =
    [0xb1c0_12a1_10ca_0001, 0xb1c0_12a1_10ca_0002, 0xb1c0_12a1_10ca_0003, 0xb1c0_12a1_10ca_0004];
const CASES_PER_SEED: usize = 8;

#[test]
fn persisted_full_lifecycle_traces_refine_independent_hierarchical_model() {
    for seed in TRACE_SEEDS {
        let mut random = Generator::new(seed);
        for case in 0..CASES_PER_SEED {
            run_case(seed, case, &mut random);
        }
    }
}

fn run_case(seed: u64, case: usize, random: &mut Generator) {
    let mut context = CaseContext::new(seed, case, random);
    context.setup_tree();
    let first = context.begin_primary();
    context.observe_primary(first);
    context.retry_ambiguously(first);
    context.cancel_held();
    context.settle_exact();
    context.overrun_and_close(random);
}

struct CaseContext {
    fixture: Fixture,
    ledger: BudgetLedger,
    model: ReferenceModel,
    runner: Runner,
    child: peritus_types::BudgetId,
    grandchild: peritus_types::BudgetId,
    sibling: peritus_types::BudgetId,
    unknown: peritus_types::BudgetId,
    ceiling: u64,
}

impl CaseContext {
    fn new(seed: u64, case: usize, random: &mut Generator) -> Self {
        let mut fixture = Fixture::new();
        let child = fixture.budget_id();
        let grandchild = fixture.budget_id();
        let sibling = fixture.budget_id();
        let unknown = fixture.budget_id();
        let root_limit = amount(300, 600, 900, 40, 20);
        Self {
            ledger: fixture.ledger(root_limit),
            model: ReferenceModel::new(fixture.root_id, fixture.revision, root_limit),
            fixture,
            runner: Runner::new(seed, case),
            child,
            grandchild,
            sibling,
            unknown,
            ceiling: random.bounded(5) + 4,
        }
    }

    fn setup_tree(&mut self) {
        self.reject(
            &BudgetCommand::Close(self.fixture.root_id),
            BudgetErrorKind::InvalidAccountPhase,
        );
        self.reject(&BudgetCommand::Seal(self.unknown), BudgetErrorKind::UnknownBudget);
        self.allocate(self.child, self.fixture.root_id, amount(220, 440, 660, 30, 15));
        let allocation_replay = BudgetCommand::AllocateChild(self.child_request(
            self.child,
            self.fixture.root_id,
            amount(220, 440, 660, 30, 15),
        ));
        self.accept_replay(&allocation_replay, BudgetReceiptKind::Idempotent);
        self.reject(
            &BudgetCommand::AllocateChild(self.child_request(
                self.child,
                self.fixture.root_id,
                amount(1, 1, 1, 1, 1),
            )),
            BudgetErrorKind::DuplicateBudgetConflict,
        );
        self.allocate(self.grandchild, self.child, amount(160, 320, 480, 20, 10));
        self.allocate(self.sibling, self.fixture.root_id, amount(20, 40, 60, 2, 1));
        let seal = BudgetCommand::Seal(self.sibling);
        let expected = self.model.seal(self.sibling);
        self.accept(&seal, expected);
        self.accept_replay(&seal, BudgetReceiptKind::Idempotent);
        let blocked = self.fixture.request(self.sibling, attempt(false), execution(1));
        self.reject(&BudgetCommand::Begin(blocked), BudgetErrorKind::AccountNotOpen);
        let close = BudgetCommand::Close(self.sibling);
        let expected = self.model.close(self.sibling);
        self.accept(&close, expected);
    }

    fn begin_primary(&mut self) -> BudgetRequest {
        let first = self.fixture.request(self.grandchild, attempt(false), execution(self.ceiling));
        let begin = BudgetCommand::Begin(first);
        let expected = self.model.begin(first);
        self.accept(&begin, expected);
        self.accept_replay(&begin, BudgetReceiptKind::Idempotent);
        let conflicting = BudgetRequest::new(
            first.reservation_id(),
            first.budget_id(),
            first.revision(),
            first.action_id(),
            digest(99),
            first.consume_now(),
            first.reserve(),
        );
        self.reject(
            &BudgetCommand::Begin(conflicting),
            BudgetErrorKind::DuplicateReservationConflict,
        );
        let activate_first = activate(first, 2);
        let expected = self.model.activate(first, digest(2));
        self.accept(&activate_first, expected);
        self.accept_replay(&activate_first, BudgetReceiptKind::Idempotent);
        self.reject(&activate(first, 3), BudgetErrorKind::BindingMismatch);
        self.reject(
            &BudgetCommand::CancelHeld(reference(first, 4)),
            BudgetErrorKind::InvalidReservationPhase,
        );
        first
    }

    fn observe_primary(&mut self, first: BudgetRequest) {
        let interim = observe(first, 5, execution(1), UsageFinality::Interim);
        let expected = self.model.observe(first, execution(1), digest(5), UsageFinality::Interim);
        self.accept(&interim, expected);
        self.accept_replay(&interim, BudgetReceiptKind::Idempotent);
        self.reject(
            &observe(first, 6, execution(1), UsageFinality::Interim),
            BudgetErrorKind::BindingMismatch,
        );
        self.reject(
            &observe(first, 5, BudgetAmounts::zero(), UsageFinality::Interim),
            BudgetErrorKind::NonmonotonicObservation,
        );
        let final_usage = execution(self.ceiling - 1);
        let final_observation = observe(first, 7, final_usage, UsageFinality::Final);
        let expected = self.model.observe(first, final_usage, digest(7), UsageFinality::Final);
        self.accept(&final_observation, expected);
        self.accept_replay(&final_observation, BudgetReceiptKind::Idempotent);
        self.reject(
            &BudgetCommand::SettleExact(reference(first, 8)),
            BudgetErrorKind::InvalidReservationPhase,
        );
    }

    fn retry_ambiguously(&mut self, first: BudgetRequest) {
        let retry = BudgetRequest::new(
            self.fixture.reservation_id(),
            self.grandchild,
            self.fixture.revision,
            first.action_id(),
            first.action_digest(),
            attempt(true),
            execution(self.ceiling - 1),
        );
        let begin = BudgetCommand::Begin(retry);
        let expected = self.model.begin(retry);
        self.accept(&begin, expected);
        let activation = activate(retry, 9);
        let expected = self.model.activate(retry, digest(9));
        self.accept(&activation, expected);
        let expected =
            self.model.consume_remainder(retry, digest(10), ReservationPhase::SettledAmbiguous);
        let ambiguous = AmbiguousFinalization::new(reference(retry, 10));
        let finalize = BudgetCommand::FinalizeAmbiguous(ambiguous);
        self.accept(&finalize, expected);
        self.accept_replay(&finalize, BudgetReceiptKind::Idempotent);
    }

    fn cancel_held(&mut self) {
        let held = fresh_request(&mut self.fixture, self.grandchild, 11, execution(2));
        let begin = BudgetCommand::Begin(held);
        let expected = self.model.begin(held);
        self.accept(&begin, expected);
        self.reject(
            &BudgetCommand::FinalizeAmbiguous(AmbiguousFinalization::new(reference(held, 12))),
            BudgetErrorKind::InvalidReservationPhase,
        );
        let forged = peritus_budget::ReservationReference::new(
            held.reservation_id(),
            self.fixture.action_id(),
            held.action_digest(),
            digest(12),
        );
        self.reject(&BudgetCommand::CancelHeld(forged), BudgetErrorKind::BindingMismatch);
        let cancel = BudgetCommand::CancelHeld(reference(held, 12));
        let expected = self.model.cancel(held, digest(12));
        self.accept(&cancel, expected);
        self.accept_replay(&cancel, BudgetReceiptKind::Idempotent);
        self.reject(&activate(held, 13), BudgetErrorKind::InvalidReservationPhase);
    }

    fn settle_exact(&mut self) {
        let exact = fresh_request(&mut self.fixture, self.grandchild, 14, execution(3));
        let begin = BudgetCommand::Begin(exact);
        let expected = self.model.begin(exact);
        self.accept(&begin, expected);
        let activation = activate(exact, 15);
        let expected = self.model.activate(exact, digest(15));
        self.accept(&activation, expected);
        let expected =
            self.model.consume_remainder(exact, digest(16), ReservationPhase::SettledExact);
        let settle = BudgetCommand::SettleExact(reference(exact, 16));
        self.accept(&settle, expected);
        self.accept_replay(&settle, BudgetReceiptKind::Idempotent);
    }

    fn overrun_and_close(&mut self, random: &mut Generator) {
        let overrun =
            fresh_request(&mut self.fixture, self.grandchild, 17, execution(self.ceiling));
        let begin = BudgetCommand::Begin(overrun);
        let expected = self.model.begin(overrun);
        self.accept(&begin, expected);
        let activation = activate(overrun, 18);
        let expected = self.model.activate(overrun, digest(18));
        self.accept(&activation, expected);
        let reported = execution(self.ceiling + random.bounded(3) + 1);
        let overrun_observation = observe(overrun, 19, reported, UsageFinality::Interim);
        let expected = self.model.overrun(overrun, reported, digest(19), UsageFinality::Interim);
        self.accept(&overrun_observation, expected);
        self.accept_replay(&overrun_observation, BudgetReceiptKind::OverrunFaulted);
        self.reject(
            &observe(overrun, 19, execution(self.ceiling + 9), UsageFinality::Interim),
            BudgetErrorKind::InvalidReservationPhase,
        );
        let grandchild_close = BudgetCommand::Close(self.grandchild);
        let expected = self.model.close(self.grandchild);
        self.accept(&grandchild_close, expected);
        self.accept_replay(&grandchild_close, BudgetReceiptKind::Idempotent);
        let child_close = BudgetCommand::Close(self.child);
        let expected = self.model.close(self.child);
        self.accept(&child_close, expected);
        let root_close = BudgetCommand::Close(self.fixture.root_id);
        let expected = self.model.close(self.fixture.root_id);
        self.accept(&root_close, expected);
    }

    fn allocate(
        &mut self,
        child: peritus_types::BudgetId,
        parent: peritus_types::BudgetId,
        limit: BudgetAmounts,
    ) {
        let expected = self.model.allocate(child, parent, self.fixture.revision, limit);
        self.accept(
            &BudgetCommand::AllocateChild(self.child_request(child, parent, limit)),
            expected,
        );
    }

    const fn child_request(
        &self,
        child: peritus_types::BudgetId,
        parent: peritus_types::BudgetId,
        limit: BudgetAmounts,
    ) -> ChildBudgetRequest {
        ChildBudgetRequest::new(child, parent, self.fixture.revision, BudgetLimits::new(limit))
    }

    fn accept(&mut self, command: &BudgetCommand, expected: ReceiptModel) {
        let point = self.runner.next();
        let transition = self.ledger.transition(*command).unwrap_or_else(|error| {
            panic!(
                "seed {:#x} case {} step {} expected acceptance: {error:?}",
                point.seed, point.case, point.step
            )
        });
        expected.assert_exact(transition.receipt(), &point);
        let next = transition.into_ledger();
        self.model.assert_matches(&next, &point);
        self.ledger = next;
    }

    fn accept_replay(&mut self, command: &BudgetCommand, kind: BudgetReceiptKind) {
        let expected = self.model.replay(command, kind);
        self.accept(command, expected);
    }

    fn reject(&mut self, command: &BudgetCommand, kind: BudgetErrorKind) {
        let point = self.runner.next();
        let expected = self.model.rejected(command, kind);
        let error = self.ledger.transition(*command).expect_err("generated rejection");
        expected.assert_exact(error, &point);
        self.model.assert_matches(&self.ledger, &point);
    }
}
