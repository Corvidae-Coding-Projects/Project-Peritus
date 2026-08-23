//! Deterministic lifecycle, fencing, time, and concurrency-plan tests.

mod support;

use peritus_leases::{
    AcquireLease, ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, HolderLossEvidence,
    HolderQuiescenceEvidence, LeaseAggregate, LeaseClaim, LeaseDuration, LeaseError, LeasePhase,
    LeaseTransitionOutcome, ReconcileLease, ReconciliationCorrelation, ReconciliationDimension,
    ReconciliationDisposition, ReconciliationObservation, ReleaseLease, RenewLease, RevokeLease,
    ScopeDimension,
};
use peritus_types::Generation;
use support::{
    FixtureIds, accepted, command, evidence, instant, mint, next_epoch, recover_rejection,
};

#[test]
fn competing_acquisitions_and_renewals_require_one_cas_winner() {
    let ids = FixtureIds::new();
    let available = mint(&ids);
    let first = accepted(available.acquire(AcquireLease::new(
        command(2),
        ids.holder(),
        LeaseDuration::new(50).expect("duration"),
        instant(10),
    )));
    let second = accepted(mint(&ids).acquire(AcquireLease::new(
        command(3),
        ids.other_holder(),
        LeaseDuration::new(50).expect("duration"),
        instant(10),
    )));
    assert_eq!(first.record().before_version(), second.record().before_version());
    assert_eq!(first.record().after_version(), second.record().after_version());

    let active = first.into_next();
    assert_eq!(active.phase(), LeasePhase::Active);
    let active = recover_rejection(
        active.acquire(AcquireLease::new(
            command(4),
            ids.other_holder(),
            LeaseDuration::new(50).expect("duration"),
            instant(11),
        )),
        LeaseError::IllegalPhase { expected: LeasePhase::Available, actual: LeasePhase::Active },
    );

    let claim = active.active().expect("active").claim();
    let first_renewal = accepted(active.renew(RenewLease::new(
        command(5),
        claim,
        LeaseDuration::new(100).expect("duration"),
        instant(20),
    )));
    let competing_renewal = accepted(support::active(&ids).renew(RenewLease::new(
        command(6),
        claim,
        LeaseDuration::new(100).expect("duration"),
        instant(20),
    )));
    assert_eq!(
        first_renewal.record().before_version(),
        competing_renewal.record().before_version()
    );
    let renewed = first_renewal.into_next();
    let _renewed = recover_rejection(
        renewed.renew(RenewLease::new(
            command(7),
            claim,
            LeaseDuration::new(200).expect("duration"),
            instant(21),
        )),
        LeaseError::ClaimVersionMismatch,
    );
}

#[test]
fn expiry_equality_fences_and_safe_reconciliation_unblocks_new_holder() {
    let ids = FixtureIds::new();
    let active = support::active(&ids);
    let old_claim = active.active().expect("active").claim();
    let fenced = accepted(active.expire(ExpireLease::new(command(8), instant(60)))).into_next();
    assert_eq!(fenced.phase(), LeasePhase::Reconciling);
    assert_eq!(fenced.generation().get(), old_claim.generation().get() + 1);
    let fenced = assert_old_claim_rejected(fenced, old_claim, 90, 61);
    let fenced = recover_rejection(
        fenced.release(ReleaseLease::new(command(9), old_claim, instant(61), None)),
        LeaseError::IllegalPhase { expected: LeasePhase::Active, actual: LeasePhase::Reconciling },
    );

    let correlation = fenced.reconciliation().expect("reconciling").correlation();
    let reconciled = accepted(fenced.reconcile(ReconcileLease::new(
        command(10),
        instant(61),
        ReconciliationObservation::new(
            correlation,
            ReconciliationDisposition::SafeToAcquire {
                holder_quiescence: evidence(1),
                resource_safety: evidence(2),
            },
        ),
    )))
    .into_next();
    assert_eq!(reconciled.phase(), LeasePhase::Available);
    let next = accepted(reconciled.acquire(AcquireLease::new(
        command(11),
        ids.other_holder(),
        LeaseDuration::new(10).expect("duration"),
        instant(62),
    )))
    .into_next();
    assert_ne!(next.active().expect("active").claim().generation(), old_claim.generation());
}

#[test]
fn release_holder_loss_revocation_and_discontinuity_all_fence() {
    let ids = FixtureIds::new();
    let active = support::active(&ids);
    let claim = active.active().expect("active").claim();
    let released = accepted(active.release(ReleaseLease::new(
        command(12),
        claim,
        instant(20),
        Some(HolderQuiescenceEvidence::new(claim, evidence(3))),
    )))
    .into_next();
    assert_eq!(released.phase(), LeasePhase::Available);
    assert_ne!(released.generation(), claim.generation());
    let _released = assert_old_claim_rejected(released, claim, 91, 21);

    let active = support::active(&ids);
    let claim = active.active().expect("active").claim();
    let lost = accepted(active.fence_holder_loss(FenceHolderLoss::new(
        command(13),
        instant(20),
        HolderLossEvidence::new(claim, evidence(4)),
    )))
    .into_next();
    assert_eq!(lost.phase(), LeasePhase::Reconciling);
    let _lost = assert_old_claim_rejected(lost, claim, 92, 21);

    let active = support::active(&ids);
    let claim = active.active().expect("active").claim();
    let revoked =
        accepted(active.revoke(RevokeLease::new(command(14), claim, instant(20), evidence(5))))
            .into_next();
    assert_eq!(revoked.phase(), LeasePhase::Reconciling);
    let _revoked = assert_old_claim_rejected(revoked, claim, 93, 21);

    let active = support::active(&ids);
    let claim = active.active().expect("active").claim();
    let discontinuous = accepted(
        active.fence_clock_discontinuity(FenceClockDiscontinuity::new(command(15), next_epoch(1))),
    )
    .into_next();
    assert_eq!(discontinuous.phase(), LeasePhase::Reconciling);
    assert_eq!(discontinuous.authority_time().epoch(), next_epoch(0).epoch());
    let _discontinuous = assert_old_claim_rejected(discontinuous, claim, 94, 21);
}

#[test]
fn ordinary_time_regression_and_early_expiry_fail_closed() {
    let ids = FixtureIds::new();
    let active = support::active(&ids);
    let active = recover_rejection(
        active.expire(ExpireLease::new(command(16), instant(59))),
        LeaseError::LeaseNotExpired,
    );
    let active = recover_rejection(
        active.acquire(AcquireLease::new(
            command(17),
            ids.other_holder(),
            LeaseDuration::new(1).expect("duration"),
            instant(9),
        )),
        LeaseError::IllegalPhase { expected: LeasePhase::Available, actual: LeasePhase::Active },
    );
    let claim = active.active().expect("active").claim();
    let _active = recover_rejection(
        active.renew(RenewLease::new(
            command(18),
            claim,
            LeaseDuration::new(100).expect("duration"),
            instant(9),
        )),
        LeaseError::ClockRegression,
    );
}

#[test]
fn dirty_and_indeterminate_reconciliation_quarantine_without_guessing() {
    let ids = FixtureIds::new();
    for (index, disposition) in [
        ReconciliationDisposition::Dirty { evidence_id: evidence(10) },
        ReconciliationDisposition::Indeterminate { evidence_id: evidence(11) },
    ]
    .into_iter()
    .enumerate()
    {
        let fenced = accepted(support::active(&ids).expire(ExpireLease::new(
            command(u8::try_from(50 + index).expect("command")),
            instant(60),
        )))
        .into_next();
        let correlation = fenced.reconciliation().expect("reconciling").correlation();
        let quarantined = accepted(fenced.reconcile(ReconcileLease::new(
            command(u8::try_from(60 + index).expect("command")),
            instant(61),
            ReconciliationObservation::new(correlation, disposition),
        )))
        .into_next();
        assert_eq!(quarantined.phase(), LeasePhase::Quarantined);
        assert_eq!(quarantined.quarantine().expect("quarantine").disposition(), disposition);
        let failure = match quarantined.acquire(AcquireLease::new(
            command(u8::try_from(70 + index).expect("command")),
            ids.other_holder(),
            LeaseDuration::new(10).expect("duration"),
            instant(62),
        )) {
            LeaseTransitionOutcome::Accepted(_) => {
                panic!("quarantined acquisition unexpectedly succeeded")
            }
            LeaseTransitionOutcome::Rejected(failure) => failure,
        };
        assert!(matches!(
            failure.error(),
            LeaseError::IllegalPhase {
                expected: LeasePhase::Available,
                actual: LeasePhase::Quarantined,
            }
        ));
    }
}

#[test]
fn every_reconciliation_correlation_mismatch_rejects_without_state_change() {
    let ids = FixtureIds::new();
    let mut fenced =
        accepted(support::active(&ids).expire(ExpireLease::new(command(80), instant(60))))
            .into_next();
    let expected = fenced.reconciliation().expect("reconciling").correlation();
    let wrong_scope =
        peritus_leases::LeaseScope::new(ids.other_workspace, ids.resource, ids.environment);
    let cases = [
        (
            ReconciliationCorrelation::new(
                wrong_scope,
                expected.fenced_generation(),
                expected.prior_holder(),
            ),
            ReconciliationDimension::Scope(ScopeDimension::Workspace),
        ),
        (
            ReconciliationCorrelation::new(
                expected.scope(),
                Generation::new(expected.fenced_generation().get() + 1).expect("generation"),
                expected.prior_holder(),
            ),
            ReconciliationDimension::FencedGeneration,
        ),
        (
            ReconciliationCorrelation::new(
                expected.scope(),
                expected.fenced_generation(),
                ids.other_holder(),
            ),
            ReconciliationDimension::PriorHolder,
        ),
    ];
    let before = support::snapshot(&fenced);
    for (index, (correlation, dimension)) in cases.into_iter().enumerate() {
        let result = fenced.reconcile(ReconcileLease::new(
            command(u8::try_from(81 + index).expect("command")),
            instant(61),
            ReconciliationObservation::new(
                correlation,
                ReconciliationDisposition::SafeToAcquire {
                    holder_quiescence: evidence(12),
                    resource_safety: evidence(13),
                },
            ),
        ));
        let failure = match result {
            LeaseTransitionOutcome::Accepted(_) => {
                panic!("mismatched reconciliation unexpectedly succeeded")
            }
            LeaseTransitionOutcome::Rejected(failure) => failure,
        };
        assert_eq!(failure.error(), &LeaseError::ReconciliationMismatch(dimension));
        fenced = failure.into_aggregate();
        assert_eq!(support::snapshot(&fenced), before);
    }
}

fn assert_old_claim_rejected(
    aggregate: LeaseAggregate,
    claim: LeaseClaim,
    command_byte: u8,
    tick: u64,
) -> LeaseAggregate {
    let before = support::snapshot(&aggregate);
    let result = aggregate.renew(RenewLease::new(
        command(command_byte),
        claim,
        LeaseDuration::new(100).expect("duration"),
        instant(tick),
    ));
    let failure = match result {
        LeaseTransitionOutcome::Accepted(_) => panic!("old claim unexpectedly renewed"),
        LeaseTransitionOutcome::Rejected(failure) => failure,
    };
    assert!(matches!(
        failure.error(),
        LeaseError::IllegalPhase { expected: LeasePhase::Active, .. }
    ));
    assert_eq!(support::snapshot(failure.aggregate()), before);
    failure.into_aggregate()
}
