//! REF-C0-B1-COMMIT-ONCE boundary fixtures for held-cancellation claims.

mod support;

use peritus_budget::{
    AmbiguousFinalization, BudgetAmounts, BudgetCommand, BudgetErrorKind, BudgetOperation,
    BudgetReceiptKind, ReservationPhase, ReservationReference,
};
use support::{Fixture, accepted, activate, digest, reference};

const fn reserve() -> BudgetAmounts {
    BudgetAmounts::from_units(3, 5, 7, 0, 0)
}

#[test]
fn forged_unknown_and_mismatched_claims_cannot_release_capacity() {
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(BudgetAmounts::from_units(6, 10, 14, 1, 0));
    let request =
        fixture.request(fixture.root_id, BudgetAmounts::from_units(0, 0, 0, 1, 0), reserve());
    let ledger = accepted(&ledger, BudgetCommand::Begin(request));
    let before = ledger.account(fixture.root_id).expect("root before forged claims");

    let unknown = ReservationReference::new(
        fixture.reservation_id(),
        request.action_id(),
        request.action_digest(),
        digest(2),
    );
    assert_eq!(
        ledger.transition(BudgetCommand::CancelHeld(unknown)).expect_err("unknown claim").kind(),
        BudgetErrorKind::UnknownReservation
    );

    let wrong_action = ReservationReference::new(
        request.reservation_id(),
        fixture.action_id(),
        request.action_digest(),
        digest(2),
    );
    assert_eq!(
        ledger
            .transition(BudgetCommand::CancelHeld(wrong_action))
            .expect_err("mismatched claim")
            .kind(),
        BudgetErrorKind::BindingMismatch
    );

    let wrong_digest = ReservationReference::new(
        request.reservation_id(),
        request.action_id(),
        digest(99),
        digest(2),
    );
    assert_eq!(
        ledger
            .transition(BudgetCommand::CancelHeld(wrong_digest))
            .expect_err("malformed digest claim")
            .kind(),
        BudgetErrorKind::BindingMismatch
    );
    assert_eq!(ledger.account(fixture.root_id).expect("unchanged root"), before);
    assert_eq!(
        ledger.reservation(request.reservation_id()).expect("held reservation").phase(),
        ReservationPhase::Held
    );
}

#[test]
fn cancellation_replay_is_exact_and_changed_evidence_is_not_a_second_release() {
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(BudgetAmounts::from_units(6, 10, 14, 1, 0));
    let request =
        fixture.request(fixture.root_id, BudgetAmounts::from_units(0, 0, 0, 1, 0), reserve());
    let ledger = accepted(&ledger, BudgetCommand::Begin(request));
    let command = BudgetCommand::CancelHeld(reference(request, 2));
    let applied = ledger.transition(command).expect("logical cancellation");
    assert_eq!(applied.receipt().operation(), BudgetOperation::CancelHeld);
    assert_eq!(applied.receipt().kind(), BudgetReceiptKind::Applied);
    assert_eq!(applied.receipt().released(), reserve());
    let ledger = applied.into_ledger();
    let after = ledger.account(fixture.root_id).expect("after cancellation");

    let replay = ledger.transition(command).expect("exact replay");
    assert_eq!(replay.receipt().kind(), BudgetReceiptKind::Idempotent);
    assert!(replay.receipt().released().is_zero());
    assert_eq!(replay.ledger().account(fixture.root_id).expect("replay root"), after);

    let changed_evidence = BudgetCommand::CancelHeld(reference(request, 3));
    assert_eq!(
        ledger.transition(changed_evidence).expect_err("different evidence is not replay").kind(),
        BudgetErrorKind::InvalidReservationPhase
    );
    assert_eq!(ledger.account(fixture.root_id).expect("unchanged after conflict"), after);
}

#[test]
fn active_or_indeterminate_history_cannot_be_recast_as_held_cancellation() {
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(BudgetAmounts::from_units(6, 10, 14, 1, 0));
    let request =
        fixture.request(fixture.root_id, BudgetAmounts::from_units(0, 0, 0, 1, 0), reserve());
    let ledger = accepted(&ledger, BudgetCommand::Begin(request));
    let ledger = accepted(&ledger, activate(request, 2));
    let active = ledger.account(fixture.root_id).expect("active accounting");
    assert_eq!(
        ledger
            .transition(BudgetCommand::CancelHeld(reference(request, 3)))
            .expect_err("active claim")
            .kind(),
        BudgetErrorKind::InvalidReservationPhase
    );
    assert_eq!(ledger.account(fixture.root_id).expect("active unchanged"), active);

    let ledger = accepted(
        &ledger,
        BudgetCommand::FinalizeAmbiguous(AmbiguousFinalization::new(reference(request, 4))),
    );
    let settled = ledger.account(fixture.root_id).expect("indeterminate charged");
    assert_eq!(
        ledger
            .transition(BudgetCommand::CancelHeld(reference(request, 5)))
            .expect_err("indeterminate history")
            .kind(),
        BudgetErrorKind::InvalidReservationPhase
    );
    assert_eq!(ledger.account(fixture.root_id).expect("settled unchanged"), settled);
    assert_eq!(
        ledger.reservation(request.reservation_id()).expect("ambiguous tombstone").phase(),
        ReservationPhase::SettledAmbiguous
    );
}
