//! Persisted-seed generated traces checked against an independent scalar high-water model.

mod support;

use peritus_budget::{
    BudgetAmounts, BudgetCommand, BudgetErrorKind, BudgetReceiptKind, ReservationPhase,
    UsageFinality,
};
use support::{Fixture, accepted, activate, observe};

const TRACE_SEEDS: [u64; 4] =
    [0x11c0_12b0_0d5e_ed01, 0x51a7_e5af_37c0_0002, 0xa11c_e5ed_1d3e_0003, 0xf00d_cafe_5eed_0004];
const CASES_PER_SEED: usize = 64;
const STEPS_PER_CASE: usize = 24;

enum Scenario {
    Decrease,
    ExactReplay,
    ConflictingEvidence,
    Increase,
    Overrun,
}

struct Coverage(u8);

impl Coverage {
    const fn new() -> Self {
        Self(0)
    }

    const fn bit(scenario: Scenario) -> u8 {
        1 << scenario as u8
    }

    const fn mark(&mut self, scenario: Scenario) {
        self.0 |= Self::bit(scenario);
    }

    const fn contains(&self, scenario: Scenario) -> bool {
        self.0 & Self::bit(scenario) != 0
    }
}

#[test]
fn persisted_seed_traces_refine_independent_high_water_model() {
    let mut coverage = Coverage::new();
    for seed in TRACE_SEEDS {
        let mut random = Generator::new(seed);
        for case in 0..CASES_PER_SEED {
            run_case(seed, case, &mut random, &mut coverage);
        }
    }
    assert!(coverage.contains(Scenario::Decrease), "persisted traces must exercise decreases");
    assert!(
        coverage.contains(Scenario::ExactReplay),
        "persisted traces must exercise exact replay"
    );
    assert!(
        coverage.contains(Scenario::ConflictingEvidence),
        "persisted traces must exercise conflicting evidence"
    );
    assert!(coverage.contains(Scenario::Increase), "persisted traces must exercise increases");
    assert!(coverage.contains(Scenario::Overrun), "persisted traces must exercise overruns");
}

fn run_case(seed: u64, case: usize, random: &mut Generator, coverage: &mut Coverage) {
    let ceiling = random.bounded(8) + 1;
    let mut fixture = Fixture::new();
    let ledger = fixture.ledger(amount(ceiling, 1));
    let request = fixture.request(fixture.root_id, amount(0, 1), amount(ceiling, 0));
    let ledger = accepted(&ledger, BudgetCommand::Begin(request));
    let mut ledger = accepted(&ledger, activate(request, 2));
    let mut observed = 0;
    let mut retained_evidence = None;
    let mut trace = Vec::new();

    for step in 0..STEPS_PER_CASE {
        let candidate = random.bounded(ceiling + 3);
        let evidence = 10 + u8::try_from(random.bounded(4)).expect("bounded evidence");
        trace.push((candidate, evidence));
        let before_account = ledger.account(fixture.root_id).expect("root snapshot");
        let before_reservation =
            ledger.reservation(request.reservation_id()).expect("reservation snapshot");
        let transition = ledger.transition(observe(
            request,
            evidence,
            amount(candidate, 0),
            UsageFinality::Interim,
        ));

        if candidate < observed {
            coverage.mark(Scenario::Decrease);
            assert_eq!(
                transition.expect_err("generated decrease").kind(),
                BudgetErrorKind::NonmonotonicObservation,
                "seed {seed:#x} case {case} step {step} trace {trace:?}"
            );
            assert_unchanged(
                &ledger,
                fixture.root_id,
                request,
                &before_account,
                &before_reservation,
            );
            continue;
        }
        if candidate == observed
            && retained_evidence.is_some()
            && retained_evidence != Some(evidence)
        {
            coverage.mark(Scenario::ConflictingEvidence);
            assert_eq!(
                transition.expect_err("generated conflicting evidence").kind(),
                BudgetErrorKind::BindingMismatch,
                "seed {seed:#x} case {case} step {step} trace {trace:?}"
            );
            assert_unchanged(
                &ledger,
                fixture.root_id,
                request,
                &before_account,
                &before_reservation,
            );
            continue;
        }

        let transition = transition.unwrap_or_else(|error| {
            panic!("seed {seed:#x} case {case} step {step} trace {trace:?}: {error:?}")
        });
        if candidate > ceiling {
            coverage.mark(Scenario::Overrun);
            assert_eq!(transition.receipt().kind(), BudgetReceiptKind::OverrunFaulted);
            ledger = transition.into_ledger();
            assert_model(
                &ledger,
                &fixture,
                request,
                &Expected { ceiling, observed: ceiling, overrun: true },
                &TracePoint { seed, case, step },
            );
            break;
        }

        let expected_kind = if candidate == observed && retained_evidence == Some(evidence) {
            coverage.mark(Scenario::ExactReplay);
            BudgetReceiptKind::Idempotent
        } else {
            if candidate > observed {
                coverage.mark(Scenario::Increase);
            }
            BudgetReceiptKind::Applied
        };
        assert_eq!(
            transition.receipt().kind(),
            expected_kind,
            "seed {seed:#x} case {case} step {step} trace {trace:?}"
        );
        observed = candidate;
        retained_evidence = Some(evidence);
        ledger = transition.into_ledger();
        assert_model(
            &ledger,
            &fixture,
            request,
            &Expected { ceiling, observed, overrun: false },
            &TracePoint { seed, case, step },
        );
    }
}

struct Expected {
    ceiling: u64,
    observed: u64,
    overrun: bool,
}

struct TracePoint {
    seed: u64,
    case: usize,
    step: usize,
}

fn assert_model(
    ledger: &peritus_budget::BudgetLedger,
    fixture: &Fixture,
    request: peritus_budget::BudgetRequest,
    expected: &Expected,
    point: &TracePoint,
) {
    let account = ledger.account(fixture.root_id).expect("root snapshot");
    assert_eq!(
        account.consumed(),
        amount(expected.observed, 1),
        "seed {:#x} case {} step {}",
        point.seed,
        point.case,
        point.step
    );
    assert_eq!(
        account.operation_reserved(),
        amount(expected.ceiling - expected.observed, 0),
        "seed {:#x} case {} step {}",
        point.seed,
        point.case,
        point.step
    );
    let reservation = ledger.reservation(request.reservation_id()).expect("reservation");
    assert_eq!(
        reservation.observed(),
        amount(expected.observed, 0),
        "seed {:#x} case {} step {}",
        point.seed,
        point.case,
        point.step
    );
    assert_eq!(
        reservation.phase(),
        if expected.overrun { ReservationPhase::OverrunFaulted } else { ReservationPhase::Active },
        "seed {:#x} case {} step {}",
        point.seed,
        point.case,
        point.step
    );
    ledger.validate().unwrap_or_else(|error| {
        panic!("seed {:#x} case {} step {}: {error:?}", point.seed, point.case, point.step)
    });
}

fn assert_unchanged(
    ledger: &peritus_budget::BudgetLedger,
    budget_id: peritus_types::BudgetId,
    request: peritus_budget::BudgetRequest,
    account: &peritus_budget::BudgetSnapshot,
    reservation: &peritus_budget::ReservationSnapshot,
) {
    assert_eq!(&ledger.account(budget_id).expect("root snapshot"), account);
    assert_eq!(
        &ledger.reservation(request.reservation_id()).expect("reservation snapshot"),
        reservation
    );
}

const fn amount(model_tokens: u64, attempts: u64) -> BudgetAmounts {
    BudgetAmounts::from_units(model_tokens, 0, 0, attempts, 0)
}

struct Generator(u64);

impl Generator {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    const fn bounded(&mut self, upper: u64) -> u64 {
        self.0 =
            self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 % upper
    }
}
