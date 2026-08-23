//! Adversarial lifecycle, overflow, and exhaustion tests.

mod support;

use peritus_budget::{
    ArithmeticKind, BudgetAccountPhase, BudgetAmounts, BudgetCommand, BudgetDimension,
    BudgetErrorKind, BudgetRecovery, BudgetRequest, ReservationPhase, UsageFinality,
};
use support::{Fixture, accepted, activate, observe, reference};

#[test]
fn exact_settlement_consumes_the_remaining_ceiling_and_replays_once() {
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(BudgetAmounts::from_units(8, 0, 0, 1, 0));
    let request = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(8, 0, 0, 0, 0),
    );
    let ledger = accepted(&ledger, BudgetCommand::Begin(request));
    let ledger = accepted(&ledger, activate(request, 2));
    let ledger = accepted(
        &ledger,
        observe(request, 3, BudgetAmounts::from_units(3, 0, 0, 0, 0), UsageFinality::Interim),
    );
    let command = BudgetCommand::SettleExact(reference(request, 4));
    let first = ledger.transition(command).expect("exact settlement");
    assert_eq!(first.receipt().charged(), BudgetAmounts::from_units(5, 0, 0, 0, 0));
    let ledger = first.into_ledger();
    assert_eq!(
        ledger.reservation(request.reservation_id()).expect("reservation").phase(),
        ReservationPhase::SettledExact
    );
    let replay = ledger.transition(command).expect("exact settlement replay");
    assert!(replay.receipt().charged().is_zero());
}

#[test]
fn close_rejects_live_work_and_seal_blocks_only_new_work() {
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(BudgetAmounts::from_units(4, 0, 0, 1, 0));
    let request = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(4, 0, 0, 0, 0),
    );
    let ledger = accepted(&ledger, BudgetCommand::Begin(request));
    assert_eq!(
        ledger.transition(BudgetCommand::Close(fixture.root_id)).expect_err("live close").kind(),
        BudgetErrorKind::InvalidAccountPhase
    );
    let ledger = accepted(&ledger, BudgetCommand::Seal(fixture.root_id));
    assert_eq!(
        ledger.account(fixture.root_id).expect("root").phase(),
        BudgetAccountPhase::Draining
    );
    assert_eq!(
        ledger.transition(BudgetCommand::Close(fixture.root_id)).expect_err("live close").kind(),
        BudgetErrorKind::OutstandingWork
    );
    let new_request = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::zero(),
    );
    assert_eq!(
        ledger.transition(BudgetCommand::Begin(new_request)).expect_err("sealed begin").kind(),
        BudgetErrorKind::AccountNotOpen
    );
    let ledger = accepted(&ledger, BudgetCommand::CancelHeld(reference(request, 2)));
    let ledger = accepted(&ledger, BudgetCommand::Close(fixture.root_id));
    assert_eq!(ledger.account(fixture.root_id).expect("root").phase(), BudgetAccountPhase::Closed);
}

#[test]
fn request_arithmetic_overflow_is_typed_and_never_partially_charges() {
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(BudgetAmounts::from_units(u64::MAX, 0, 0, 1, 0));
    let request = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(u64::MAX, 0, 0, 1, 0),
        BudgetAmounts::from_units(1, 0, 0, 0, 0),
    );
    let error =
        ledger.transition(BudgetCommand::Begin(request)).expect_err("unrepresentable request sum");
    assert_eq!(error.kind(), BudgetErrorKind::Arithmetic);
    assert_eq!(error.code(), "PERITUS-BUDGET-013");
    assert_eq!(error.recovery(), BudgetRecovery::CallerCorrectable);
    assert!(ledger.account(fixture.root_id).expect("root").consumed().is_zero());
    assert_eq!(ledger.reservation_count(), 0);
}

#[test]
fn reducer_overflow_is_typed_and_state_preserving_in_every_dimension() {
    let mut fixture = Fixture::new();
    let maximum = BudgetAmounts::from_units(u64::MAX, u64::MAX, u64::MAX, u64::MAX, u64::MAX);
    let ledger = fixture.ledger(maximum);
    let cases = [
        (
            BudgetDimension::ModelTokens,
            BudgetAmounts::from_units(u64::MAX, 0, 0, 1, 0),
            BudgetAmounts::from_units(1, 0, 0, 0, 0),
        ),
        (
            BudgetDimension::ProviderCostMicrounits,
            BudgetAmounts::from_units(0, u64::MAX, 0, 1, 0),
            BudgetAmounts::from_units(0, 1, 0, 0, 0),
        ),
        (
            BudgetDimension::ActiveEffectMilliseconds,
            BudgetAmounts::from_units(0, 0, u64::MAX, 1, 0),
            BudgetAmounts::from_units(0, 0, 1, 0, 0),
        ),
        (
            BudgetDimension::Attempts,
            BudgetAmounts::from_units(0, 0, 0, u64::MAX, 0),
            BudgetAmounts::from_units(0, 0, 0, 1, 0),
        ),
        (
            BudgetDimension::Retries,
            BudgetAmounts::from_units(0, 0, 0, 1, u64::MAX),
            BudgetAmounts::from_units(0, 0, 0, 0, 1),
        ),
    ];
    for (dimension, consume_now, reserve) in cases {
        let request = BudgetRequest::new(
            fixture.reservation_id(),
            fixture.root_id,
            fixture.revision,
            fixture.action_id(),
            fixture.action_digest,
            consume_now,
            reserve,
        );
        let error = ledger
            .transition(BudgetCommand::Begin(request))
            .expect_err("unrepresentable begin must fail before mutation");
        assert_eq!(error.kind(), BudgetErrorKind::Arithmetic);
        let arithmetic = error.arithmetic_error().expect("arithmetic detail");
        assert_eq!(arithmetic.kind(), ArithmeticKind::Overflow);
        assert_eq!(arithmetic.dimension(), dimension);
        assert!(ledger.account(fixture.root_id).expect("root").consumed().is_zero());
        assert_eq!(ledger.reservation_count(), 0);
    }
}

#[test]
fn zero_limits_are_valid_and_immediately_exhausted() {
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(BudgetAmounts::zero());
    ledger.validate().expect("zero-limit root is valid");
    let request = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::zero(),
    );
    let error =
        ledger.transition(BudgetCommand::Begin(request)).expect_err("attempt dimension exhausted");
    assert_eq!(error.kind(), BudgetErrorKind::InsufficientBudget);
}
