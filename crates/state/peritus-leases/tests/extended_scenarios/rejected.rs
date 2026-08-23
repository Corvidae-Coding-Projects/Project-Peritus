//! Rejected transition families checked for exact error, state, and move-only ownership.

use crate::extended_reference::ExpectedUseOutput;
use crate::support::{FixtureIds, accepted, action, command, evidence, instant};
use peritus_leases::{
    AcquireLease, ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, HolderLossEvidence,
    HolderQuiescenceEvidence, LeaseAggregate, LeaseDuration, LeaseError, LeasePhase,
    LeaseTransitionOutcome, LeaseUseOutcome, PolicyIntersectionDimension, ReconcileLease,
    ReconciliationCorrelation, ReconciliationDisposition, ReconciliationObservation, ReleaseLease,
    RenewLease, RevokeLease, UseLease,
};
use peritus_types::Generation;

pub fn run(seed: u8, case: u8) {
    let ids = FixtureIds::new();
    let active = crate::support::active(&ids);
    let claim = active.active().expect("active fixture").claim();
    let wrong_claim = accepted(crate::support::mint(&ids).acquire(AcquireLease::new(
        command(240),
        ids.other_holder(),
        LeaseDuration::new(50).expect("duration"),
        instant(10),
    )))
    .into_next()
    .active()
    .expect("other active")
    .claim();
    let before = crate::support::snapshot(&active);
    let command_id = command(case * 16 + seed);
    if case == 5 {
        return reject_use(seed, &ids, active, claim, &before, command_id);
    }
    let (result, expected) =
        rejected_transition(case, active, claim, wrong_claim, &ids, command_id, seed);
    let failure = match result {
        LeaseTransitionOutcome::Accepted(_) => {
            panic!("seed {seed} case {case}: accepted rejection")
        }
        LeaseTransitionOutcome::Rejected(value) => value,
    };
    assert_eq!(failure.error(), &expected, "seed {seed} case {case}: error");
    assert_eq!(
        crate::support::snapshot(failure.aggregate()),
        before,
        "seed {seed} case {case}: rejected state"
    );
}

fn rejected_transition(
    case: u8,
    active: LeaseAggregate,
    claim: peritus_leases::LeaseClaim,
    wrong_claim: peritus_leases::LeaseClaim,
    ids: &FixtureIds,
    command_id: peritus_types::CommandId,
    seed: u8,
) -> (LeaseTransitionOutcome, LeaseError) {
    match case {
        0 => (
            active.release(ReleaseLease::new(
                command_id,
                claim,
                instant(20),
                Some(HolderQuiescenceEvidence::new(wrong_claim, evidence(seed))),
            )),
            LeaseError::HolderQuiescenceMismatch,
        ),
        1 => (
            active.fence_holder_loss(FenceHolderLoss::new(
                command_id,
                instant(20),
                HolderLossEvidence::new(wrong_claim, evidence(seed)),
            )),
            LeaseError::HolderLossMismatch,
        ),
        2 => (
            active.revoke(RevokeLease::new(command_id, wrong_claim, instant(20), evidence(seed))),
            LeaseError::ClaimHolderMismatch,
        ),
        3 => {
            (active.expire(ExpireLease::new(command_id, instant(20))), LeaseError::LeaseNotExpired)
        }
        4 => (
            active.fence_clock_discontinuity(FenceClockDiscontinuity::new(command_id, instant(10))),
            LeaseError::NoClockDiscontinuity,
        ),
        6 => reconcile_in_active(active, ids, command_id, seed),
        7 => (
            active.acquire(AcquireLease::new(
                command_id,
                ids.other_holder(),
                LeaseDuration::new(20).expect("duration"),
                instant(20),
            )),
            LeaseError::IllegalPhase {
                expected: LeasePhase::Available,
                actual: LeasePhase::Active,
            },
        ),
        _ => (
            active.renew(RenewLease::new(
                command_id,
                wrong_claim,
                LeaseDuration::new(100).expect("duration"),
                instant(20),
            )),
            LeaseError::ClaimHolderMismatch,
        ),
    }
}

fn reconcile_in_active(
    active: LeaseAggregate,
    ids: &FixtureIds,
    command_id: peritus_types::CommandId,
    seed: u8,
) -> (LeaseTransitionOutcome, LeaseError) {
    let correlation =
        ReconciliationCorrelation::new(ids.scope(), active.generation(), ids.holder());
    (
        active.reconcile(ReconcileLease::new(
            command_id,
            instant(20),
            ReconciliationObservation::new(
                correlation,
                ReconciliationDisposition::Indeterminate { evidence_id: evidence(seed) },
            ),
        )),
        LeaseError::IllegalPhase { expected: LeasePhase::Reconciling, actual: LeasePhase::Active },
    )
}

fn reject_use(
    seed: u8,
    ids: &FixtureIds,
    active: LeaseAggregate,
    claim: peritus_leases::LeaseClaim,
    before: &crate::support::AggregateSnapshot,
    command_id: peritus_types::CommandId,
) {
    let wrong_generation = Generation::new(2).expect("wrong generation");
    let capability = crate::support::capability_use(
        ids,
        &crate::support::CapabilityUseFixture::new(
            ids.actor,
            ids.environment,
            ids.workspace,
            wrong_generation,
            ids.resource,
            instant(20),
            action(seed),
        ),
    );
    let expected_command = ExpectedUseOutput::new(command_id, claim, instant(20), &capability);
    let failure =
        match active.authorize_use(UseLease::new(command_id, claim, instant(20), capability)) {
            LeaseUseOutcome::Accepted(_) => panic!("seed {seed}: stale-generation use accepted"),
            LeaseUseOutcome::Rejected(value) => value,
        };
    assert_eq!(
        failure.error(),
        &LeaseError::PolicyIntersectionMismatch(PolicyIntersectionDimension::Generation),
        "seed {seed}: use rejection"
    );
    expected_command.assert_rejected_command(failure.command(), seed, "rejected-use");
    assert_eq!(
        &crate::support::snapshot(failure.aggregate()),
        before,
        "seed {seed}: use rejection state"
    );
}
