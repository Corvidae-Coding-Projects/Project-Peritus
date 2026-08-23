//! Internal representation-boundary tests for reserved fencing arithmetic.

use super::*;
use super::boundary_reference::{RetirementBinding, RetirementTransitionReference};
use crate::state::{ActiveLease, ReconciliationState};
use crate::{
    AcquireLease, ExpireLease, FenceCause, LeaseDuration, LeaseHolder, LeaseScope, ReconcileLease,
    RenewLease, LeaseTransitionOutcome, ReconciliationCorrelation, ReconciliationDisposition,
    ReconciliationObservation, RetirementReason,
};
use peritus_policy::{AuthorityInstant, AuthorityTimeState};
use peritus_types::{
    ActorId, CommandId, EnvironmentId, EvidenceId, Generation, ResourceId, RevisionNumber,
    SessionId, WorkspaceId,
};

fn identifier(byte: u8) -> [u8; 16] { [byte; 16] }

fn command(byte: u8) -> CommandId { CommandId::new(identifier(byte)).expect("command") }

fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}

fn scope() -> LeaseScope {
    LeaseScope::new(
        WorkspaceId::new(identifier(1)).expect("workspace"),
        ResourceId::new(identifier(2)).expect("resource"),
        EnvironmentId::new(identifier(3)).expect("environment"),
    )
}

fn holder() -> LeaseHolder {
    LeaseHolder::new(
        ActorId::new(identifier(4)).expect("actor"),
        SessionId::new(identifier(5)).expect("session"),
    )
}

fn active_at(generation: Generation, version: u64, claim_version: u64) -> LeaseAggregate {
    LeaseAggregate::from_parts(
        scope(),
        generation,
        RevisionNumber::new(version).expect("version"),
        AuthorityTimeState::new(instant(10)),
        LeaseState::Active(ActiveLease {
            holder: holder(),
            claim_version: RevisionNumber::new(claim_version).expect("claim version"),
            issued_at: instant(10),
            expires_at: instant(20),
        }),
    )
}

fn recover_rejection(
    result: LeaseTransitionOutcome,
    expected: LeaseError,
) -> LeaseAggregate {
    match result {
        LeaseTransitionOutcome::Accepted(_) => panic!("command unexpectedly succeeded"),
        LeaseTransitionOutcome::Rejected(failure) => {
            assert_eq!(failure.error(), &expected);
            failure.into_aggregate()
        }
    }
}

fn accepted(outcome: LeaseTransitionOutcome) -> LeaseTransition {
    match outcome {
        LeaseTransitionOutcome::Accepted(value) => value,
        LeaseTransitionOutcome::Rejected(failure) => {
            panic!("command rejected: {:?}", failure.error())
        }
    }
}

#[test]
fn final_representable_version_is_reserved_for_fencing() {
    let active = active_at(Generation::first(), u64::MAX - 1, 1);
    let claim = active.active().expect("active").claim();
    let active = recover_rejection(
        active.renew(RenewLease::new(
            command(10),
            claim,
            LeaseDuration::new(20).expect("duration"),
            instant(11),
        )),
        LeaseError::VersionExhausted,
    );
    let retired = accepted(active.expire(ExpireLease::new(command(11), instant(20)))).into_next();
    assert_eq!(retired.version().get(), u64::MAX);
    assert_eq!(retired.retirement_reason(), Some(RetirementReason::VersionExhausted));
}

#[test]
fn generation_exhaustion_retires_without_wrapping() {
    let active = active_at(Generation::new(u64::MAX).expect("maximum generation"), 2, 1);
    let retired = accepted(active.expire(ExpireLease::new(command(12), instant(20)))).into_next();
    assert_eq!(retired.generation().get(), u64::MAX);
    assert_eq!(
        retired.retirement_reason(),
        Some(RetirementReason::GenerationExhausted)
    );
}

#[test]
fn claim_version_exhaustion_and_corrupt_active_max_fail_closed() {
    let active = active_at(Generation::first(), 2, u64::MAX);
    let claim = active.active().expect("active").claim();
    let _active = recover_rejection(
        active.renew(RenewLease::new(
            command(13),
            claim,
            LeaseDuration::new(20).expect("duration"),
            instant(11),
        )),
        LeaseError::ClaimVersionExhausted,
    );
    let corrupt = active_at(Generation::first(), u64::MAX, 1);
    assert_eq!(corrupt.validate(), Err(LeaseError::CorruptState));
}

#[test]
fn boundary_reconciliation_retires_before_the_reserved_final_fence_step() {
    let current_generation = Generation::new(2).expect("generation");
    let correlation = ReconciliationCorrelation::new(scope(), Generation::first(), holder());
    let reconciling = LeaseAggregate::from_parts(
        scope(),
        current_generation,
        RevisionNumber::new(u64::MAX - 2).expect("version"),
        AuthorityTimeState::new(instant(20)),
        LeaseState::Reconciling(ReconciliationState {
            correlation,
            cause: FenceCause::Expired,
        }),
    );
    let observation = ReconciliationObservation::new(
        correlation,
        ReconciliationDisposition::SafeToAcquire {
            holder_quiescence: EvidenceId::new(identifier(6)).expect("evidence"),
            resource_safety: EvidenceId::new(identifier(7)).expect("evidence"),
        },
    );
    let transition = accepted(reconciling.reconcile(ReconcileLease::new(
        command(14),
        instant(21),
        observation,
    )));
    assert_eq!(
        transition.record().kind(),
        LeaseTransitionKind::Retired(RetirementReason::VersionExhausted)
    );
    let retired = transition.into_next();
    assert_eq!(retired.version().get(), u64::MAX - 1);
    assert_eq!(retired.generation(), current_generation);
    assert_eq!(retired.retirement_reason(), Some(RetirementReason::VersionExhausted));
}

#[test]
fn acquisition_cannot_create_active_state_at_maximum_version() {
    let available = LeaseAggregate::from_parts(
        scope(),
        Generation::first(),
        RevisionNumber::new(u64::MAX - 1).expect("version"),
        AuthorityTimeState::new(instant(10)),
        LeaseState::Available,
    );
    let _available = recover_rejection(
        available.acquire(AcquireLease::new(
            command(15),
            holder(),
            LeaseDuration::new(10).expect("duration"),
            instant(10),
        )),
        LeaseError::VersionExhausted,
    );
}

#[test]
fn generated_boundary_retirements_match_independent_reference() {
    for seed in 1..=16_u8 {
        assert_generated_version_fence_retirement(seed);
        assert_generated_generation_fence_retirement(seed);
        assert_generated_reconciliation_retirement(seed);
        assert_generated_reserved_version_rejections(seed);
    }
}

fn assert_generated_version_fence_retirement(seed: u8) {
    let active = active_at(Generation::first(), u64::MAX - 1, 1);
    let command = ExpireLease::new(command(seed), instant(20 + u64::from(seed)));
    let transition = accepted(active.expire(command));
    RetirementTransitionReference {
        scope: scope(),
        before_generation: Generation::first(),
        before_version: RevisionNumber::new(u64::MAX - 1).expect("before version"),
        before_phase: LeasePhase::Active,
        command_id: command.command_id(),
        after_generation: Generation::first(),
        after_version: RevisionNumber::new(u64::MAX).expect("after version"),
        authority_epoch: Generation::first(),
        authority_tick: 20 + u64::from(seed),
        reason: RetirementReason::VersionExhausted,
        binding: RetirementBinding::Expire(command),
    }
    .assert_matches(&transition, seed, "version fence");
}

fn assert_generated_generation_fence_retirement(seed: u8) {
    let active = active_at(Generation::new(u64::MAX).expect("maximum generation"), 2, 1);
    let command = ExpireLease::new(command(seed + 16), instant(20 + u64::from(seed)));
    let transition = accepted(active.expire(command));
    RetirementTransitionReference {
        scope: scope(),
        before_generation: Generation::new(u64::MAX).expect("before generation"),
        before_version: RevisionNumber::new(2).expect("before version"),
        before_phase: LeasePhase::Active,
        command_id: command.command_id(),
        after_generation: Generation::new(u64::MAX).expect("after generation"),
        after_version: RevisionNumber::new(3).expect("after version"),
        authority_epoch: Generation::first(),
        authority_tick: 20 + u64::from(seed),
        reason: RetirementReason::GenerationExhausted,
        binding: RetirementBinding::Expire(command),
    }
    .assert_matches(&transition, seed, "generation fence");
}

fn assert_generated_reconciliation_retirement(seed: u8) {
    let current_generation = Generation::new(2).expect("generation");
    let correlation =
        ReconciliationCorrelation::new(scope(), Generation::first(), holder());
    let reconciling = LeaseAggregate::from_parts(
        scope(),
        current_generation,
        RevisionNumber::new(u64::MAX - 2).expect("version"),
        AuthorityTimeState::new(instant(20)),
        LeaseState::Reconciling(ReconciliationState {
            correlation,
            cause: FenceCause::Expired,
        }),
    );
    let observation = ReconciliationObservation::new(
        correlation,
        ReconciliationDisposition::SafeToAcquire {
            holder_quiescence: EvidenceId::new(identifier(seed)).expect("evidence"),
            resource_safety: EvidenceId::new(identifier(seed + 16)).expect("evidence"),
        },
    );
    let command = ReconcileLease::new(
        command(seed + 32),
        instant(21 + u64::from(seed)),
        observation,
    );
    let transition = accepted(reconciling.reconcile(command));
    RetirementTransitionReference {
        scope: scope(),
        before_generation: current_generation,
        before_version: RevisionNumber::new(u64::MAX - 2).expect("before version"),
        before_phase: LeasePhase::Reconciling,
        command_id: command.command_id(),
        after_generation: current_generation,
        after_version: RevisionNumber::new(u64::MAX - 1).expect("after version"),
        authority_epoch: Generation::first(),
        authority_tick: 21 + u64::from(seed),
        reason: RetirementReason::VersionExhausted,
        binding: RetirementBinding::Reconcile(command),
    }
    .assert_matches(&transition, seed, "reconciliation");
}

fn assert_generated_reserved_version_rejections(seed: u8) {
    let available = LeaseAggregate::from_parts(
        scope(),
        Generation::first(),
        RevisionNumber::new(u64::MAX - 1).expect("version"),
        AuthorityTimeState::new(instant(10)),
        LeaseState::Available,
    );
    let available = recover_rejection(
        available.acquire(AcquireLease::new(
            command(seed + 48),
            holder(),
            LeaseDuration::new(10).expect("duration"),
            instant(10),
        )),
        LeaseError::VersionExhausted,
    );
    assert_eq!(available.phase(), LeasePhase::Available, "seed {seed}: acquire phase");
    assert_eq!(available.version().get(), u64::MAX - 1, "seed {seed}: acquire version");

    let active = active_at(Generation::first(), u64::MAX - 1, 1);
    let claim = active.active().expect("active").claim();
    let active = recover_rejection(
        active.renew(RenewLease::new(
            command(seed + 64),
            claim,
            LeaseDuration::new(20).expect("duration"),
            instant(11),
        )),
        LeaseError::VersionExhausted,
    );
    assert_eq!(active.phase(), LeasePhase::Active, "seed {seed}: renew phase");
    assert_eq!(active.version().get(), u64::MAX - 1, "seed {seed}: renew version");
    assert_eq!(
        active.active().expect("active").claim(),
        claim,
        "seed {seed}: renew claim"
    );
}
