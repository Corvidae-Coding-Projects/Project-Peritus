//! Bounded reference-model and adversarial trace tests for reconciliation.

mod support;

use peritus_budget::{
    BudgetAmounts, BudgetCommand, BudgetErrorKind, BudgetReceiptKind, ReservationPhase,
    UsageFinality,
};
use std::cmp::Ordering;
use support::{Fixture, accepted, activate, observe};

fn active_reservation(
    ceiling: u64,
) -> (Fixture, peritus_budget::BudgetLedger, peritus_budget::BudgetRequest) {
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(BudgetAmounts::from_units(ceiling, 0, 0, 1, 0));
    let request = fixture.request(
        fixture.root_id,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(ceiling, 0, 0, 0, 0),
    );
    let ledger = accepted(&ledger, BudgetCommand::Begin(request));
    let ledger = accepted(&ledger, activate(request, 2));
    (fixture, ledger, request)
}

#[test]
fn bounded_reordered_trace_matches_independent_high_water_model() {
    const CEILING: u64 = 4;
    for first in 0..=CEILING {
        for second in 0..=CEILING {
            let (fixture, ledger, request) = active_reservation(CEILING);
            let ledger = accepted(
                &ledger,
                observe(
                    request,
                    3,
                    BudgetAmounts::from_units(first, 0, 0, 0, 0),
                    UsageFinality::Interim,
                ),
            );
            let transition = ledger.transition(observe(
                request,
                4,
                BudgetAmounts::from_units(second, 0, 0, 0, 0),
                UsageFinality::Interim,
            ));
            match second.cmp(&first) {
                Ordering::Less => {
                    assert_eq!(
                        transition.expect_err("decreasing sample").kind(),
                        BudgetErrorKind::NonmonotonicObservation
                    );
                    assert_eq!(
                        ledger
                            .reservation(request.reservation_id())
                            .expect("reservation")
                            .observed(),
                        BudgetAmounts::from_units(first, 0, 0, 0, 0)
                    );
                }
                Ordering::Equal => {
                    assert_eq!(
                        transition
                            .expect_err("different evidence is not an exact duplicate")
                            .kind(),
                        BudgetErrorKind::BindingMismatch
                    );
                    let exact_replay = ledger
                        .transition(observe(
                            request,
                            3,
                            BudgetAmounts::from_units(second, 0, 0, 0, 0),
                            UsageFinality::Interim,
                        ))
                        .expect("equal sample with retained evidence is idempotent");
                    assert_eq!(exact_replay.receipt().kind(), BudgetReceiptKind::Idempotent);
                    assert_eq!(exact_replay.into_ledger(), ledger);
                }
                Ordering::Greater => {
                    let transition = transition.expect("monotonic sample");
                    assert_eq!(transition.receipt().kind(), BudgetReceiptKind::Applied);
                    let ledger = transition.into_ledger();
                    assert_eq!(
                        ledger.account(fixture.root_id).expect("root").consumed(),
                        BudgetAmounts::from_units(second, 0, 0, 1, 0)
                    );
                    assert_eq!(
                        ledger.account(fixture.root_id).expect("root").operation_reserved(),
                        BudgetAmounts::from_units(CEILING - second, 0, 0, 0, 0)
                    );
                    ledger.validate().expect("model trace remains valid");
                }
            }
        }
    }
}

#[test]
fn every_bounded_final_report_partitions_ceiling_without_refund() {
    const CEILING: u64 = 4;
    for reported in 0..=CEILING {
        let (fixture, ledger, request) = active_reservation(CEILING);
        let transition = ledger
            .transition(observe(
                request,
                3,
                BudgetAmounts::from_units(reported, 0, 0, 0, 0),
                UsageFinality::Final,
            ))
            .expect("bounded final report");
        assert_eq!(
            transition.receipt().released(),
            BudgetAmounts::from_units(CEILING - reported, 0, 0, 0, 0)
        );
        let ledger = transition.into_ledger();
        assert_eq!(
            ledger.account(fixture.root_id).expect("root").consumed(),
            BudgetAmounts::from_units(reported, 0, 0, 1, 0)
        );
        assert_eq!(
            ledger.reservation(request.reservation_id()).expect("reservation").phase(),
            ReservationPhase::SettledFinal
        );
    }
}

#[test]
fn one_over_ceiling_is_conservative_and_exact_replay_is_stable() {
    let (_fixture, ledger, request) = active_reservation(4);
    let command =
        observe(request, 3, BudgetAmounts::from_units(5, 0, 0, 0, 0), UsageFinality::Interim);
    let first = ledger.transition(command).expect("overrun transition");
    assert_eq!(first.receipt().kind(), BudgetReceiptKind::OverrunFaulted);
    assert_eq!(first.receipt().charged(), BudgetAmounts::from_units(4, 0, 0, 0, 0));
    let ledger = first.into_ledger();
    assert_eq!(
        ledger.reservation(request.reservation_id()).expect("reservation").finality(),
        Some(UsageFinality::Interim)
    );
    let replay = ledger.transition(command).expect("exact overrun replay");
    assert_eq!(replay.receipt().kind(), BudgetReceiptKind::OverrunFaulted);
    assert!(replay.receipt().charged().is_zero());
    assert_eq!(replay.into_ledger(), ledger);
}
