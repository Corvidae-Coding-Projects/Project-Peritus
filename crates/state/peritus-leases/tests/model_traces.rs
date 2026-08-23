//! Deterministic generated traces compared with an independent lease reference model.

mod support;

use peritus_leases::{
    AcquireLease, ExpireLease, LeaseAggregate, LeaseDuration, LeaseError, LeasePhase,
    LeaseTransitionOutcome, ReconcileLease, ReconciliationCorrelation, ReconciliationDisposition,
    ReconciliationObservation, ReleaseLease, RenewLease,
};
use peritus_types::Generation;
use support::{FixtureIds, accepted, command, evidence, instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReferencePhase {
    Available,
    Active,
    Reconciling,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReferenceState {
    phase: ReferencePhase,
    generation: u64,
    version: u64,
    claim_version: Option<u64>,
    expires_at: Option<u64>,
}

impl ReferenceState {
    const fn minted() -> Self {
        Self {
            phase: ReferencePhase::Available,
            generation: 1,
            version: 1,
            claim_version: None,
            expires_at: None,
        }
    }

    fn assert_refines(self, aggregate: &LeaseAggregate, seed: u64, step: u8) {
        let phase = match aggregate.phase() {
            LeasePhase::Available => ReferencePhase::Available,
            LeasePhase::Active => ReferencePhase::Active,
            LeasePhase::Reconciling => ReferencePhase::Reconciling,
            other => panic!("seed {seed} step {step}: unexpected phase {other:?}"),
        };
        assert_eq!(phase, self.phase, "seed {seed} step {step}: phase");
        assert_eq!(
            aggregate.generation().get(),
            self.generation,
            "seed {seed} step {step}: generation"
        );
        assert_eq!(aggregate.version().get(), self.version, "seed {seed} step {step}: version");
        let active = aggregate.active().map(peritus_leases::ActiveLeaseView::claim);
        assert_eq!(
            active.map(|claim| claim.claim_version().get()),
            self.claim_version,
            "seed {seed} step {step}: claim version"
        );
        assert_eq!(
            active.map(|claim| claim.expires_at().tick_millis()),
            self.expires_at,
            "seed {seed} step {step}: expiry"
        );
    }
}

const fn next_choice(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    *state
}

#[test]
fn generated_traces_refine_independent_model_after_success_and_rejection() {
    for seed in 1..=64_u64 {
        let ids = FixtureIds::new();
        let mut aggregate = support::mint(&ids);
        let mut reference = ReferenceState::minted();
        let mut generator = seed;
        for step in 1..=32_u8 {
            let choice = next_choice(&mut generator);
            let tick = 10 + u64::from(step);
            aggregate = match reference.phase {
                ReferencePhase::Available => {
                    available_step(&ids, aggregate, &mut reference, choice, tick, step, seed)
                }
                ReferencePhase::Active => {
                    active_step(&ids, aggregate, &mut reference, choice, tick, step, seed)
                }
                ReferencePhase::Reconciling => {
                    reconciling_step(&ids, aggregate, &mut reference, choice, tick, step, seed)
                }
            };
            reference.assert_refines(&aggregate, seed, step);
        }
    }
}

fn available_step(
    ids: &FixtureIds,
    aggregate: LeaseAggregate,
    reference: &mut ReferenceState,
    choice: u64,
    tick: u64,
    step: u8,
    seed: u64,
) -> LeaseAggregate {
    if choice.is_multiple_of(2) {
        let aggregate = accepted(aggregate.acquire(AcquireLease::new(
            command(step),
            ids.holder(),
            LeaseDuration::new(50).expect("duration"),
            instant(tick),
        )))
        .into_next();
        reference.phase = ReferencePhase::Active;
        reference.version += 1;
        reference.claim_version = Some(1);
        reference.expires_at = Some(tick + 50);
        aggregate
    } else {
        let before = support::snapshot(&aggregate);
        let correlation =
            ReconciliationCorrelation::new(ids.scope(), Generation::first(), ids.holder());
        let result = aggregate.reconcile(ReconcileLease::new(
            command(step),
            instant(tick),
            ReconciliationObservation::new(
                correlation,
                ReconciliationDisposition::SafeToAcquire {
                    holder_quiescence: evidence(1),
                    resource_safety: evidence(2),
                },
            ),
        ));
        let failure = match result {
            LeaseTransitionOutcome::Accepted(_) => {
                panic!("rejected reconcile unexpectedly succeeded")
            }
            LeaseTransitionOutcome::Rejected(failure) => failure,
        };
        assert!(matches!(
            failure.error(),
            LeaseError::IllegalPhase {
                expected: LeasePhase::Reconciling,
                actual: LeasePhase::Available,
            }
        ));
        assert_eq!(
            support::snapshot(failure.aggregate()),
            before,
            "seed {seed} step {step}: rejected reconcile"
        );
        failure.into_aggregate()
    }
}

fn active_step(
    ids: &FixtureIds,
    aggregate: LeaseAggregate,
    reference: &mut ReferenceState,
    choice: u64,
    tick: u64,
    step: u8,
    seed: u64,
) -> LeaseAggregate {
    let before = support::snapshot(&aggregate);
    match choice % 4 {
        0 => {
            let claim = aggregate.active().expect("active").claim();
            let old_expiry = reference.expires_at.expect("reference expiry");
            let duration = old_expiry - tick + 10;
            let aggregate = accepted(aggregate.renew(RenewLease::new(
                command(step),
                claim,
                LeaseDuration::new(duration).expect("duration"),
                instant(tick),
            )))
            .into_next();
            reference.version += 1;
            reference.claim_version = reference.claim_version.map(|value| value + 1);
            reference.expires_at = Some(old_expiry + 10);
            aggregate
        }
        1 => {
            let claim = aggregate.active().expect("active").claim();
            let aggregate = accepted(aggregate.release(ReleaseLease::new(
                command(step),
                claim,
                instant(tick),
                None,
            )))
            .into_next();
            reference.phase = ReferencePhase::Reconciling;
            reference.generation += 1;
            reference.version += 1;
            reference.claim_version = None;
            reference.expires_at = None;
            aggregate
        }
        2 => {
            let failure = match aggregate.expire(ExpireLease::new(command(step), instant(tick))) {
                LeaseTransitionOutcome::Accepted(_) => {
                    panic!("early expiry unexpectedly succeeded")
                }
                LeaseTransitionOutcome::Rejected(failure) => failure,
            };
            assert_eq!(failure.error(), &LeaseError::LeaseNotExpired);
            assert_eq!(
                support::snapshot(failure.aggregate()),
                before,
                "seed {seed} step {step}: rejected expiry"
            );
            failure.into_aggregate()
        }
        _ => {
            let result = aggregate.acquire(AcquireLease::new(
                command(step),
                ids.other_holder(),
                LeaseDuration::new(10).expect("duration"),
                instant(tick),
            ));
            let failure = match result {
                LeaseTransitionOutcome::Accepted(_) => {
                    panic!("active acquisition unexpectedly succeeded")
                }
                LeaseTransitionOutcome::Rejected(failure) => failure,
            };
            assert!(matches!(failure.error(), LeaseError::IllegalPhase { .. }));
            assert_eq!(
                support::snapshot(failure.aggregate()),
                before,
                "seed {seed} step {step}: rejected acquire"
            );
            failure.into_aggregate()
        }
    }
}

fn reconciling_step(
    ids: &FixtureIds,
    aggregate: LeaseAggregate,
    reference: &mut ReferenceState,
    choice: u64,
    tick: u64,
    step: u8,
    seed: u64,
) -> LeaseAggregate {
    let before = support::snapshot(&aggregate);
    match choice % 3 {
        0 => {
            let correlation = aggregate.reconciliation().expect("reconciling").correlation();
            let aggregate = accepted(aggregate.reconcile(ReconcileLease::new(
                command(step),
                instant(tick),
                ReconciliationObservation::new(
                    correlation,
                    ReconciliationDisposition::SafeToAcquire {
                        holder_quiescence: evidence(3),
                        resource_safety: evidence(4),
                    },
                ),
            )))
            .into_next();
            reference.phase = ReferencePhase::Available;
            reference.version += 1;
            aggregate
        }
        1 => {
            let expected = aggregate.reconciliation().expect("reconciling").correlation();
            let wrong = ReconciliationCorrelation::new(
                expected.scope(),
                Generation::new(expected.fenced_generation().get() + 1).expect("wrong generation"),
                expected.prior_holder(),
            );
            let result = aggregate.reconcile(ReconcileLease::new(
                command(step),
                instant(tick),
                ReconciliationObservation::new(
                    wrong,
                    ReconciliationDisposition::SafeToAcquire {
                        holder_quiescence: evidence(5),
                        resource_safety: evidence(6),
                    },
                ),
            ));
            let failure = match result {
                LeaseTransitionOutcome::Accepted(_) => {
                    panic!("mismatched reconciliation unexpectedly succeeded")
                }
                LeaseTransitionOutcome::Rejected(failure) => failure,
            };
            assert!(matches!(failure.error(), LeaseError::ReconciliationMismatch(_)));
            assert_eq!(
                support::snapshot(failure.aggregate()),
                before,
                "seed {seed} step {step}: mismatch"
            );
            failure.into_aggregate()
        }
        _ => {
            let result = aggregate.acquire(AcquireLease::new(
                command(step),
                ids.other_holder(),
                LeaseDuration::new(10).expect("duration"),
                instant(tick),
            ));
            let failure = match result {
                LeaseTransitionOutcome::Accepted(_) => {
                    panic!("reconciling acquisition unexpectedly succeeded")
                }
                LeaseTransitionOutcome::Rejected(failure) => failure,
            };
            assert!(matches!(failure.error(), LeaseError::IllegalPhase { .. }));
            assert_eq!(
                support::snapshot(failure.aggregate()),
                before,
                "seed {seed} step {step}: blocked takeover"
            );
            failure.into_aggregate()
        }
    }
}
