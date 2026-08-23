//! Deterministic branch-order rejection traces against the independent exact oracle.

mod reference_support;
mod support;

use peritus_budget::{
    Activation, BudgetAmounts, BudgetCommand, BudgetErrorKind, BudgetLedger, BudgetReceiptKind,
    BudgetRequest,
};
use reference_support::{
    ReceiptModel, ReferenceModel, Runner, amount, attempt, execution, fresh_request,
};
use support::{Fixture, digest, reference};

const TRACE_SEED: u64 = 0xb1c0_12a1_0bad_0001;

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "The persisted trace intentionally keeps branch precedence in one auditable sequence"
)]
fn branch_ordered_rejections_preserve_the_exact_full_ledger() {
    let mut fixture = Fixture::new();
    let limit = amount(10, 20, 30, 4, 2);
    let mut ledger = fixture.ledger(limit);
    let mut model = ReferenceModel::new(fixture.root_id, fixture.revision, limit);
    let mut runner = Runner::new(TRACE_SEED, 0);

    let unknown_budget = fixture.budget_id();
    reject(
        &ledger,
        &model,
        &mut runner,
        &BudgetCommand::Seal(unknown_budget),
        BudgetErrorKind::UnknownBudget,
    );

    let unknown_reservation = fixture.reservation_id();
    reject(
        &ledger,
        &model,
        &mut runner,
        &BudgetCommand::Activate(Activation::new(
            unknown_reservation,
            fixture.action_id,
            fixture.action_digest,
            digest(20),
        )),
        BudgetErrorKind::UnknownReservation,
    );

    let overflow = BudgetRequest::new(
        fixture.reservation_id(),
        fixture.root_id,
        fixture.revision,
        fixture.action_id(),
        digest(21),
        amount(u64::MAX, 0, 0, 1, 0),
        amount(1, 0, 0, 0, 0),
    );
    reject(
        &ledger,
        &model,
        &mut runner,
        &BudgetCommand::Begin(overflow),
        BudgetErrorKind::Arithmetic,
    );

    let empty = fixture.request(fixture.root_id, BudgetAmounts::zero(), BudgetAmounts::zero());
    reject(
        &ledger,
        &model,
        &mut runner,
        &BudgetCommand::Begin(empty),
        BudgetErrorKind::EmptyRequest,
    );

    let invalid_attempt = fixture.request(fixture.root_id, BudgetAmounts::zero(), execution(1));
    reject(
        &ledger,
        &model,
        &mut runner,
        &BudgetCommand::Begin(invalid_attempt),
        BudgetErrorKind::InvalidAttemptAccounting,
    );

    let insufficient = fixture.request(fixture.root_id, attempt(false), amount(11, 21, 31, 0, 0));
    reject(
        &ledger,
        &model,
        &mut runner,
        &BudgetCommand::Begin(insufficient),
        BudgetErrorKind::InsufficientBudget,
    );

    let root_id = fixture.root_id;
    let held = fresh_request(&mut fixture, root_id, 22, execution(1));
    let begin = BudgetCommand::Begin(held);
    let expected = model.begin(held);
    ledger = accept(&ledger, &model, &mut runner, &begin, expected);

    let unresolved_retry = BudgetRequest::new(
        fixture.reservation_id(),
        held.budget_id(),
        held.revision(),
        held.action_id(),
        held.action_digest(),
        attempt(true),
        execution(1),
    );
    reject(
        &ledger,
        &model,
        &mut runner,
        &BudgetCommand::Begin(unresolved_retry),
        BudgetErrorKind::PriorAttemptUnresolved,
    );

    let seal = BudgetCommand::Seal(fixture.root_id);
    let expected = model.seal(fixture.root_id);
    ledger = accept(&ledger, &model, &mut runner, &seal, expected);
    reject(
        &ledger,
        &model,
        &mut runner,
        &BudgetCommand::Close(fixture.root_id),
        BudgetErrorKind::OutstandingWork,
    );

    let blocked = fresh_request(&mut fixture, root_id, 23, execution(1));
    reject(
        &ledger,
        &model,
        &mut runner,
        &BudgetCommand::Begin(blocked),
        BudgetErrorKind::AccountNotOpen,
    );

    let cancel = BudgetCommand::CancelHeld(reference(held, 24));
    let expected = model.cancel(held, digest(24));
    ledger = accept(&ledger, &model, &mut runner, &cancel, expected);
    let close = BudgetCommand::Close(fixture.root_id);
    let expected = model.close(fixture.root_id);
    let ledger = accept(&ledger, &model, &mut runner, &close, expected);
    model.assert_matches(&ledger, &runner.next());
}

fn accept(
    ledger: &BudgetLedger,
    model: &ReferenceModel,
    runner: &mut Runner,
    command: &BudgetCommand,
    expected: ReceiptModel,
) -> BudgetLedger {
    let point = runner.next();
    let transition = ledger.transition(*command).unwrap_or_else(|error| {
        panic!(
            "seed {:#x} case {} step {} expected acceptance: {error:?}",
            point.seed, point.case, point.step
        )
    });
    expected.assert_exact(transition.receipt(), &point);
    let next = transition.into_ledger();
    model.assert_matches(&next, &point);
    next
}

fn reject(
    ledger: &BudgetLedger,
    model: &ReferenceModel,
    runner: &mut Runner,
    command: &BudgetCommand,
    kind: BudgetErrorKind,
) {
    let point = runner.next();
    let expected = model.rejected(command, kind);
    let error = ledger.transition(*command).expect_err("generated rejection");
    expected.assert_exact(error, &point);
    model.assert_matches(ledger, &point);
}

#[test]
fn replay_receipts_are_exact_and_nonconsuming() {
    let fixture = Fixture::new();
    let limit = amount(8, 16, 24, 2, 1);
    let ledger = fixture.ledger(limit);
    let mut model = ReferenceModel::new(fixture.root_id, fixture.revision, limit);
    let mut runner = Runner::new(TRACE_SEED, 1);
    let request = BudgetRequest::new(
        peritus_types::BudgetReservationId::new([31; 16]).expect("nonzero reservation id"),
        fixture.root_id,
        fixture.revision,
        fixture.action_id,
        fixture.action_digest,
        attempt(false),
        execution(2),
    );
    let command = BudgetCommand::Begin(request);
    let expected = model.begin(request);
    let ledger = accept(&ledger, &model, &mut runner, &command, expected);
    let expected = model.replay(&command, BudgetReceiptKind::Idempotent);
    let ledger = accept(&ledger, &model, &mut runner, &command, expected);
    model.assert_matches(&ledger, &runner.next());
}
