//! Exhaustive legal and illegal command coverage across every publicly reachable lease phase.

mod support;

use peritus_leases::{
    AcquireLease, ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, HolderLossEvidence,
    LeaseAggregate, LeaseDuration, LeaseError, LeasePhase, LeaseTransitionKind,
    LeaseTransitionOutcome, LeaseUseOutcome, ReconcileLease, ReconciliationCorrelation,
    ReconciliationDisposition, ReconciliationObservation, ReleaseLease, RenewLease, RevokeLease,
    UseLease,
};
use peritus_types::Generation;
use support::{FixtureIds, accepted, action, command, digest, evidence, instant, next_epoch};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandCase {
    Acquire,
    Renew,
    Release,
    Expire,
    HolderLoss,
    ClockDiscontinuity,
    Revoke,
    Reconcile,
    Use,
}

const COMMANDS: [CommandCase; 9] = [
    CommandCase::Acquire,
    CommandCase::Renew,
    CommandCase::Release,
    CommandCase::Expire,
    CommandCase::HolderLoss,
    CommandCase::ClockDiscontinuity,
    CommandCase::Revoke,
    CommandCase::Reconcile,
    CommandCase::Use,
];

const PHASES: [LeasePhase; 4] =
    [LeasePhase::Available, LeasePhase::Active, LeasePhase::Reconciling, LeasePhase::Quarantined];

#[test]
fn every_publicly_reachable_phase_command_edge_is_explicit() {
    let ids = FixtureIds::new();
    for phase in PHASES {
        for command_case in COMMANDS {
            let aggregate = aggregate_in_phase(&ids, phase);
            let before = support::snapshot(&aggregate);
            let (aggregate, result) = apply(&ids, aggregate, command_case);
            if let Some(expected) = expected_kind(phase, command_case) {
                assert_eq!(result, Ok(expected), "{phase:?} {command_case:?}: legal edge");
            } else {
                assert_eq!(
                    result,
                    Err(expected_phase_error(phase, command_case)),
                    "{phase:?} {command_case:?}: illegal edge"
                );
                assert_eq!(
                    support::snapshot(&aggregate),
                    before,
                    "{phase:?} {command_case:?}: rejection preserves state"
                );
            }
        }
    }
}

fn aggregate_in_phase(ids: &FixtureIds, phase: LeasePhase) -> LeaseAggregate {
    match phase {
        LeasePhase::Available => support::mint(ids),
        LeasePhase::Active => support::active(ids),
        LeasePhase::Reconciling => reconciling(ids),
        LeasePhase::Quarantined => quarantined(ids),
        LeasePhase::Retired => panic!("retired needs representation-boundary construction"),
    }
}

fn reconciling(ids: &FixtureIds) -> LeaseAggregate {
    let active = support::active(ids);
    let claim = active.active().expect("active fixture").claim();
    accepted(active.release(ReleaseLease::new(command(200), claim, instant(20), None))).into_next()
}

fn quarantined(ids: &FixtureIds) -> LeaseAggregate {
    let reconciling = reconciling(ids);
    let correlation = reconciling.reconciliation().expect("reconciling fixture").correlation();
    accepted(reconciling.reconcile(ReconcileLease::new(
        command(201),
        instant(21),
        ReconciliationObservation::new(
            correlation,
            ReconciliationDisposition::Dirty { evidence_id: evidence(201) },
        ),
    )))
    .into_next()
}

fn donor_claim(ids: &FixtureIds) -> peritus_leases::LeaseClaim {
    support::active(ids).active().expect("claim donor").claim()
}

fn claim_for(ids: &FixtureIds, aggregate: &LeaseAggregate) -> peritus_leases::LeaseClaim {
    aggregate.active().map_or_else(|| donor_claim(ids), peritus_leases::ActiveLeaseView::claim)
}

const fn correlation_for(
    ids: &FixtureIds,
    aggregate: &LeaseAggregate,
) -> ReconciliationCorrelation {
    match aggregate.reconciliation() {
        Some(reconciling) => reconciling.correlation(),
        None => match aggregate.quarantine() {
            Some(quarantined) => quarantined.correlation(),
            None => ReconciliationCorrelation::new(ids.scope(), Generation::first(), ids.holder()),
        },
    }
}

const fn observation_tick(phase: LeasePhase, command_case: CommandCase) -> u64 {
    match (phase, command_case) {
        (LeasePhase::Active, CommandCase::Expire) => 60,
        (LeasePhase::Active | LeasePhase::Retired, _) => 20,
        (LeasePhase::Available, _) => 11,
        (LeasePhase::Reconciling, _) => 21,
        (LeasePhase::Quarantined, _) => 22,
    }
}

fn apply(
    ids: &FixtureIds,
    aggregate: LeaseAggregate,
    command_case: CommandCase,
) -> (LeaseAggregate, Result<LeaseTransitionKind, LeaseError>) {
    let phase = aggregate.phase();
    let tick = observation_tick(phase, command_case);
    let claim = claim_for(ids, &aggregate);
    let command_id = command(100 + command_case as u8);
    match command_case {
        CommandCase::Acquire => normalize(aggregate.acquire(AcquireLease::new(
            command_id,
            ids.other_holder(),
            LeaseDuration::new(50).expect("duration"),
            instant(tick),
        ))),
        CommandCase::Renew => normalize(aggregate.renew(RenewLease::new(
            command_id,
            claim,
            LeaseDuration::new(100).expect("duration"),
            instant(tick),
        ))),
        CommandCase::Release => {
            normalize(aggregate.release(ReleaseLease::new(command_id, claim, instant(tick), None)))
        }
        CommandCase::Expire => {
            normalize(aggregate.expire(ExpireLease::new(command_id, instant(tick))))
        }
        CommandCase::HolderLoss => normalize(aggregate.fence_holder_loss(FenceHolderLoss::new(
            command_id,
            instant(tick),
            HolderLossEvidence::new(claim, evidence(202)),
        ))),
        CommandCase::ClockDiscontinuity => {
            normalize(aggregate.fence_clock_discontinuity(FenceClockDiscontinuity::new(
                command_id,
                next_epoch(tick),
            )))
        }
        CommandCase::Revoke => normalize(aggregate.revoke(RevokeLease::new(
            command_id,
            claim,
            instant(tick),
            evidence(203),
        ))),
        CommandCase::Reconcile => {
            let correlation = correlation_for(ids, &aggregate);
            normalize(aggregate.reconcile(ReconcileLease::new(
                command_id,
                instant(tick),
                ReconciliationObservation::new(
                    correlation,
                    ReconciliationDisposition::SafeToAcquire {
                        holder_quiescence: evidence(204),
                        resource_safety: evidence(205),
                    },
                ),
            )))
        }
        CommandCase::Use => normalize_use(ids, aggregate, claim, command_id, tick),
    }
}

fn normalize(
    outcome: LeaseTransitionOutcome,
) -> (LeaseAggregate, Result<LeaseTransitionKind, LeaseError>) {
    match outcome {
        LeaseTransitionOutcome::Accepted(transition) => {
            let kind = transition.record().kind();
            (transition.into_next(), Ok(kind))
        }
        LeaseTransitionOutcome::Rejected(failure) => {
            let error = *failure.error();
            (failure.into_aggregate(), Err(error))
        }
    }
}

fn normalize_use(
    ids: &FixtureIds,
    aggregate: LeaseAggregate,
    claim: peritus_leases::LeaseClaim,
    command_id: peritus_types::CommandId,
    tick: u64,
) -> (LeaseAggregate, Result<LeaseTransitionKind, LeaseError>) {
    let policy_use = support::capability_use(
        ids,
        &support::CapabilityUseFixture::new(
            ids.actor,
            ids.environment,
            ids.workspace,
            aggregate.generation(),
            ids.resource,
            instant(tick),
            action(100),
        ),
    );
    match aggregate.authorize_use(UseLease::new(command_id, claim, instant(tick), policy_use)) {
        LeaseUseOutcome::Accepted(logical_use) => {
            let (transition, _capability_use) = logical_use.into_parts();
            let kind = transition.record().kind();
            (transition.into_next(), Ok(kind))
        }
        LeaseUseOutcome::Rejected(failure) => {
            let error = *failure.error();
            let (lease_failure, _command) = failure.into_parts();
            (lease_failure.into_aggregate(), Err(error))
        }
    }
}

fn expected_kind(phase: LeasePhase, command_case: CommandCase) -> Option<LeaseTransitionKind> {
    match (phase, command_case) {
        (LeasePhase::Available, CommandCase::Acquire) => Some(LeaseTransitionKind::Acquired),
        (LeasePhase::Active, CommandCase::Renew) => Some(LeaseTransitionKind::Renewed),
        (LeasePhase::Active, CommandCase::Release) => {
            Some(LeaseTransitionKind::ReleasedReconciling)
        }
        (LeasePhase::Active, CommandCase::Expire) => Some(LeaseTransitionKind::Expired),
        (LeasePhase::Active, CommandCase::HolderLoss) => Some(LeaseTransitionKind::HolderLost),
        (LeasePhase::Active, CommandCase::ClockDiscontinuity) => {
            Some(LeaseTransitionKind::ClockDiscontinuity)
        }
        (LeasePhase::Active, CommandCase::Revoke) => Some(LeaseTransitionKind::Revoked),
        (LeasePhase::Active, CommandCase::Use) => {
            Some(LeaseTransitionKind::Used { action_id: action(100), action_digest: digest(12) })
        }
        (LeasePhase::Reconciling, CommandCase::Reconcile) => {
            Some(LeaseTransitionKind::ReconciledAvailable)
        }
        _ => None,
    }
}

const fn expected_phase_error(phase: LeasePhase, command_case: CommandCase) -> LeaseError {
    LeaseError::IllegalPhase {
        expected: match command_case {
            CommandCase::Acquire => LeasePhase::Available,
            CommandCase::Reconcile => LeasePhase::Reconciling,
            CommandCase::Renew
            | CommandCase::Release
            | CommandCase::Expire
            | CommandCase::HolderLoss
            | CommandCase::ClockDiscontinuity
            | CommandCase::Revoke
            | CommandCase::Use => LeasePhase::Active,
        },
        actual: phase,
    }
}
