//! End-to-end tests for hierarchical ledger transitions and replay safety.

mod support;

use peritus_budget::{
    AmbiguousFinalization, BudgetAccountPhase, BudgetAmounts, BudgetCommand, BudgetDimension,
    BudgetErrorKind, BudgetLimits, BudgetReceiptKind, ChildBudgetRequest, ReservationPhase,
    UsageFinality,
};
use support::{Fixture, accepted, activate, observe, reference};

#[test]
fn exact_limit_is_accepted_and_one_over_reports_all_limiting_dimensions() {
    let mut fixture = Fixture::new();
    let limits = BudgetAmounts::from_units(10, 20, 30, 1, 0);
    let ledger = fixture.ledger(limits);
    let request = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(10, 20, 30, 0, 0),
    );
    let ledger = accepted(&ledger, BudgetCommand::Begin(request));
    assert!(ledger.account(fixture.root_id).expect("root").available().is_zero());
    let ledger = accepted(&ledger, activate(request, 2));
    let ledger = accepted(
        &ledger,
        observe(request, 3, BudgetAmounts::from_units(10, 20, 30, 0, 0), UsageFinality::Final),
    );
    assert_eq!(ledger.account(fixture.root_id).expect("root").consumed(), limits);

    let mut second_fixture = Fixture::new();
    let second = second_fixture.ledger(BudgetAmounts::from_units(10, 20, 30, 1, 0));
    let too_large = second_fixture.request(
        second_fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(11, 21, 31, 0, 0),
    );
    let error = second.transition(BudgetCommand::Begin(too_large)).expect_err("one over must fail");
    assert_eq!(error.kind(), BudgetErrorKind::InsufficientBudget);
    for dimension in [
        BudgetDimension::ModelTokens,
        BudgetDimension::ProviderCostMicrounits,
        BudgetDimension::ActiveEffectMilliseconds,
    ] {
        assert!(error.limiting_dimensions().contains(dimension));
    }
}

#[test]
fn cumulative_observations_charge_only_delta_reject_decrease_and_release_final_remainder() {
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(BudgetAmounts::from_units(10, 0, 0, 1, 0));
    let request = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(10, 0, 0, 0, 0),
    );
    let ledger = accepted(&ledger, BudgetCommand::Begin(request));
    let ledger = accepted(&ledger, activate(request, 2));
    let first = ledger
        .transition(observe(
            request,
            3,
            BudgetAmounts::from_units(3, 0, 0, 0, 0),
            UsageFinality::Interim,
        ))
        .expect("partial observation");
    assert_eq!(first.receipt().charged(), BudgetAmounts::from_units(3, 0, 0, 0, 0));
    let ledger = first.into_ledger();

    let replay = ledger
        .transition(observe(
            request,
            3,
            BudgetAmounts::from_units(3, 0, 0, 0, 0),
            UsageFinality::Interim,
        ))
        .expect("duplicate high water");
    assert_eq!(replay.receipt().kind(), BudgetReceiptKind::Idempotent);
    assert_eq!(replay.receipt().charged(), BudgetAmounts::zero());
    let ledger = replay.into_ledger();

    let error = ledger
        .transition(observe(
            request,
            5,
            BudgetAmounts::from_units(2, 0, 0, 0, 0),
            UsageFinality::Interim,
        ))
        .expect_err("lower correction");
    assert_eq!(error.kind(), BudgetErrorKind::NonmonotonicObservation);
    assert_eq!(
        ledger.account(fixture.root_id).expect("root").consumed(),
        BudgetAmounts::from_units(3, 0, 0, 1, 0)
    );

    let final_transition = ledger
        .transition(observe(
            request,
            6,
            BudgetAmounts::from_units(7, 0, 0, 0, 0),
            UsageFinality::Final,
        ))
        .expect("final observation");
    assert_eq!(final_transition.receipt().charged(), BudgetAmounts::from_units(4, 0, 0, 0, 0));
    assert_eq!(final_transition.receipt().released(), BudgetAmounts::from_units(3, 0, 0, 0, 0));
    let ledger = final_transition.into_ledger();
    assert_eq!(
        ledger.account(fixture.root_id).expect("root").consumed(),
        BudgetAmounts::from_units(7, 0, 0, 1, 0)
    );
    assert_eq!(
        ledger.reservation(request.reservation_id()).expect("reservation").phase(),
        ReservationPhase::SettledFinal
    );
}

#[test]
fn held_cancel_releases_but_active_ambiguity_consumes_every_remaining_unit() {
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(BudgetAmounts::from_units(20, 0, 0, 2, 1));
    let cancelled = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(8, 0, 0, 0, 0),
    );
    let ledger = accepted(&ledger, BudgetCommand::Begin(cancelled));
    let cancel = ledger
        .transition(BudgetCommand::CancelHeld(reference(cancelled, 2)))
        .expect("held cancellation");
    assert_eq!(cancel.receipt().released(), BudgetAmounts::from_units(8, 0, 0, 0, 0));
    let ledger = cancel.into_ledger();

    let ambiguous = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 1),
        BudgetAmounts::from_units(12, 0, 0, 0, 0),
    );
    let ledger = accepted(&ledger, BudgetCommand::Begin(ambiguous));
    let ledger = accepted(&ledger, activate(ambiguous, 3));
    let ledger = accepted(
        &ledger,
        observe(ambiguous, 4, BudgetAmounts::from_units(5, 0, 0, 0, 0), UsageFinality::Interim),
    );
    let result = ledger
        .transition(BudgetCommand::FinalizeAmbiguous(AmbiguousFinalization::new(reference(
            ambiguous, 5,
        ))))
        .expect("ambiguous finalization");
    assert_eq!(result.receipt().charged(), BudgetAmounts::from_units(7, 0, 0, 0, 0));
    assert_eq!(result.receipt().released(), BudgetAmounts::zero());
    let ledger = result.into_ledger();
    assert_eq!(
        ledger.account(fixture.root_id).expect("root").consumed(),
        BudgetAmounts::from_units(12, 0, 0, 2, 1)
    );
}

#[test]
fn child_consumption_reaches_each_ancestor_once_and_close_only_releases_unused_delegation() {
    let mut fixture = Fixture::new();
    let root_id = fixture.root_id;
    let child_id = fixture.budget_id();
    let grandchild_id = fixture.budget_id();
    let ledger = fixture.ledger(BudgetAmounts::from_units(100, 0, 0, 10, 5));
    let ledger = accepted(
        &ledger,
        BudgetCommand::AllocateChild(ChildBudgetRequest::new(
            child_id,
            root_id,
            fixture.revision,
            BudgetLimits::new(BudgetAmounts::from_units(80, 0, 0, 8, 4)),
        )),
    );
    let ledger = accepted(
        &ledger,
        BudgetCommand::AllocateChild(ChildBudgetRequest::new(
            grandchild_id,
            child_id,
            fixture.revision,
            BudgetLimits::new(BudgetAmounts::from_units(50, 0, 0, 5, 2)),
        )),
    );
    let request = fixture.request(
        grandchild_id,
        BudgetAmounts::from_units(10, 0, 0, 1, 0),
        BudgetAmounts::zero(),
    );
    let ledger = accepted(&ledger, BudgetCommand::Begin(request));
    let root = ledger.account(root_id).expect("root");
    let child = ledger.account(child_id).expect("child");
    let grandchild = ledger.account(grandchild_id).expect("grandchild");
    assert_eq!(root.consumed(), BudgetAmounts::from_units(10, 0, 0, 1, 0));
    assert_eq!(root.child_delegated_remaining(), BudgetAmounts::from_units(70, 0, 0, 7, 4));
    assert_eq!(child.consumed(), BudgetAmounts::from_units(10, 0, 0, 1, 0));
    assert_eq!(child.child_delegated_remaining(), BudgetAmounts::from_units(40, 0, 0, 4, 2));
    assert_eq!(grandchild.consumed(), BudgetAmounts::from_units(10, 0, 0, 1, 0));

    let ledger = accepted(&ledger, BudgetCommand::Seal(grandchild_id));
    let close = ledger
        .transition(BudgetCommand::Close(grandchild_id))
        .expect("quiescent grandchild closes");
    assert_eq!(close.receipt().released(), BudgetAmounts::from_units(40, 0, 0, 4, 2));
    let ledger = close.into_ledger();
    assert_eq!(
        ledger.account(child_id).expect("child").child_delegated_remaining(),
        BudgetAmounts::zero()
    );
    assert_eq!(
        ledger.account(child_id).expect("child").consumed(),
        BudgetAmounts::from_units(10, 0, 0, 1, 0)
    );
    assert_eq!(
        ledger.account(root_id).expect("root").consumed(),
        BudgetAmounts::from_units(10, 0, 0, 1, 0)
    );
}

#[test]
fn above_ceiling_consumes_remaining_ceiling_faults_lineage_and_preserves_raw_report() {
    let mut fixture = Fixture::new();
    let root_id = fixture.root_id;
    let child_id = fixture.budget_id();
    let ledger = fixture.ledger(BudgetAmounts::from_units(20, 0, 0, 2, 1));
    let ledger = accepted(
        &ledger,
        BudgetCommand::AllocateChild(ChildBudgetRequest::new(
            child_id,
            root_id,
            fixture.revision,
            BudgetLimits::new(BudgetAmounts::from_units(10, 0, 0, 1, 0)),
        )),
    );
    let request = fixture.request(
        child_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(10, 0, 0, 0, 0),
    );
    let ledger = accepted(&ledger, BudgetCommand::Begin(request));
    let ledger = accepted(&ledger, activate(request, 2));
    let overrun = ledger
        .transition(observe(
            request,
            3,
            BudgetAmounts::from_units(11, 0, 0, 0, 0),
            UsageFinality::Final,
        ))
        .expect("overrun is accepted conservatively");
    assert_eq!(overrun.receipt().kind(), BudgetReceiptKind::OverrunFaulted);
    assert_eq!(overrun.receipt().charged(), BudgetAmounts::from_units(10, 0, 0, 0, 0));
    assert_eq!(overrun.receipt().reported(), Some(BudgetAmounts::from_units(11, 0, 0, 0, 0)));
    let ledger = overrun.into_ledger();
    assert_eq!(ledger.account(child_id).expect("child").phase(), BudgetAccountPhase::Faulted);
    assert_eq!(ledger.account(root_id).expect("root").phase(), BudgetAccountPhase::Faulted);
    assert_eq!(
        ledger.reservation(request.reservation_id()).expect("reservation").final_reported(),
        Some(BudgetAmounts::from_units(11, 0, 0, 0, 0))
    );

    let later =
        fixture.request(root_id, BudgetAmounts::from_units(0, 0, 0, 1, 1), BudgetAmounts::zero());
    assert_eq!(
        ledger.transition(BudgetCommand::Begin(later)).expect_err("faulted lineage").kind(),
        BudgetErrorKind::AccountNotOpen
    );
}
