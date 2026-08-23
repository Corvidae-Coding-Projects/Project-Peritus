//! Retry-lineage, immutable tombstone, and evidence-binding tests.

mod support;

use peritus_budget::{
    BudgetAmounts, BudgetCommand, BudgetErrorKind, BudgetReceiptKind, UsageFinality,
};
use peritus_types::Sha256Digest;
use support::{Fixture, accepted, activate, digest, observe};

#[test]
fn retry_requires_fresh_identity_and_charges_attempt_and_retry_before_execution() {
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(BudgetAmounts::from_units(0, 0, 0, 2, 1));
    let first = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::zero(),
    );
    let ledger = accepted(&ledger, BudgetCommand::Begin(first));
    let replay = ledger.transition(BudgetCommand::Begin(first)).expect("same identity replay");
    assert_eq!(replay.receipt().kind(), BudgetReceiptKind::Idempotent);
    let ledger = replay.into_ledger();
    let omitted_retry = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::zero(),
    );
    assert_eq!(
        ledger
            .transition(BudgetCommand::Begin(omitted_retry))
            .expect_err("fresh identity cannot omit retry charge")
            .kind(),
        BudgetErrorKind::InvalidAttemptAccounting
    );
    assert_eq!(ledger.reservation_count(), 1);
    let retry = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 1),
        BudgetAmounts::zero(),
    );
    let ledger = accepted(&ledger, BudgetCommand::Begin(retry));
    let exhausted = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 1),
        BudgetAmounts::zero(),
    );
    assert_eq!(
        ledger.transition(BudgetCommand::Begin(exhausted)).expect_err("attempt exhaustion").kind(),
        BudgetErrorKind::InsufficientBudget
    );

    let malformed = BudgetCommand::Begin(peritus_budget::BudgetRequest::new(
        fixture.reservation_id(),
        fixture.root_id,
        fixture.revision,
        fixture.action_id,
        fixture.action_digest,
        BudgetAmounts::from_units(0, 0, 0, 0, 1),
        BudgetAmounts::zero(),
    ));
    assert_eq!(
        ledger.transition(malformed).expect_err("retry without attempt").kind(),
        BudgetErrorKind::InvalidAttemptAccounting
    );
}

#[test]
fn retry_waits_for_terminal_resolution_and_duplicate_observations_bind_evidence() {
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(BudgetAmounts::from_units(4, 0, 0, 2, 1));
    let first = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(4, 0, 0, 0, 0),
    );
    let ledger = accepted(&ledger, BudgetCommand::Begin(first));
    let premature = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 1),
        BudgetAmounts::zero(),
    );
    assert_eq!(
        ledger
            .transition(BudgetCommand::Begin(premature))
            .expect_err("held prior attempt must resolve")
            .kind(),
        BudgetErrorKind::PriorAttemptUnresolved
    );

    let ledger = accepted(&ledger, activate(first, 2));
    let observation =
        observe(first, 3, BudgetAmounts::from_units(2, 0, 0, 0, 0), UsageFinality::Interim);
    let ledger = accepted(&ledger, observation);
    let replay = ledger.transition(observation).expect("exact observation replay");
    assert_eq!(replay.receipt().kind(), BudgetReceiptKind::Idempotent);
    assert_eq!(replay.into_ledger(), ledger);
    let conflicting_evidence =
        observe(first, 4, BudgetAmounts::from_units(2, 0, 0, 0, 0), UsageFinality::Interim);
    assert_eq!(
        ledger
            .transition(conflicting_evidence)
            .expect_err("equal high water with different evidence is not a replay")
            .kind(),
        BudgetErrorKind::BindingMismatch
    );
    assert_eq!(
        ledger.reservation(first.reservation_id()).expect("reservation").observation_evidence(),
        Some(digest(3))
    );
}

#[test]
fn reservation_tombstones_reject_semantic_id_reuse_and_binding_tampering() {
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(BudgetAmounts::from_units(10, 0, 0, 2, 0));
    let request = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(5, 0, 0, 0, 0),
    );
    let ledger = accepted(&ledger, BudgetCommand::Begin(request));
    let conflicting = peritus_budget::BudgetRequest::new(
        request.reservation_id(),
        request.budget_id(),
        request.revision(),
        fixture.action_id(),
        Sha256Digest::new([9; 32]),
        request.consume_now(),
        request.reserve(),
    );
    assert_eq!(
        ledger.transition(BudgetCommand::Begin(conflicting)).expect_err("semantic ID reuse").kind(),
        BudgetErrorKind::DuplicateReservationConflict
    );
    let wrong_action = peritus_budget::Activation::new(
        request.reservation_id(),
        request.action_id(),
        digest(99),
        digest(2),
    );
    assert_eq!(
        ledger
            .transition(BudgetCommand::Activate(wrong_action))
            .expect_err("digest tampering")
            .kind(),
        BudgetErrorKind::BindingMismatch
    );
}
