//! Accepted transition families checked against the exact independent oracle.

use crate::extended_reference::{
    ExpectedBinding, ExpectedTransition, ExpectedUseOutput, ReferenceState, assert_transition,
};
use crate::support::{FixtureIds, accepted, action, command, evidence, instant, next_epoch};
use peritus_leases::{
    AcquireLease, ExpireLease, FenceCause, FenceClockDiscontinuity, FenceHolderLoss,
    HolderLossEvidence, HolderQuiescenceEvidence, LeaseAggregate, LeaseDuration, LeaseTransition,
    LeaseTransitionKind, LeaseUseOutcome, MintLease, ReconcileLease, ReconciliationDisposition,
    ReconciliationObservation, ReleaseLease, RenewLease, RevokeLease, UseLease,
};

pub fn run_foundation(seed: u8) {
    let ids = FixtureIds::new();
    let mint_command = MintLease::new(command(seed), ids.scope(), instant(10));
    let minted_reference = ReferenceState::minted(ids.scope(), instant(10));
    let minted = LeaseAggregate::mint(mint_command).expect("mint transition");
    let expected_mint = ExpectedTransition::new(
        None,
        minted_reference,
        command(seed),
        LeaseTransitionKind::Minted,
        Box::new(ExpectedBinding::Mint(mint_command)),
        "mint",
    );
    assert_transition(&minted, &expected_mint, seed);
    let minted = minted.into_next();

    let acquire_command = AcquireLease::new(
        command(seed + 32),
        ids.holder(),
        LeaseDuration::new(50).expect("duration"),
        instant(10),
    );
    let active_reference = minted_reference.after_acquire(ids.holder(), instant(10), 50);
    let acquired = accepted(minted.acquire(acquire_command));
    let expected_acquire = ExpectedTransition::new(
        Some(minted_reference),
        active_reference,
        command(seed + 32),
        LeaseTransitionKind::Acquired,
        Box::new(ExpectedBinding::Acquire(acquire_command)),
        "acquire",
    );
    assert_transition(&acquired, &expected_acquire, seed);
    let active = acquired.into_next();
    let claim = active.active().expect("active claim").claim();

    let renew_command = RenewLease::new(
        command(seed + 64),
        claim,
        LeaseDuration::new(50).expect("duration"),
        instant(20),
    );
    let renewed_reference = active_reference.after_renew(instant(20), 50);
    let renewed = accepted(active.renew(renew_command));
    let expected_renew = ExpectedTransition::new(
        Some(active_reference),
        renewed_reference,
        command(seed + 64),
        LeaseTransitionKind::Renewed,
        Box::new(ExpectedBinding::Renew(renew_command)),
        "renew",
    );
    assert_transition(&renewed, &expected_renew, seed);
}

pub fn run(seed: u8, case: u8) {
    let ids = FixtureIds::new();
    let active = crate::support::active(&ids);
    let claim = active.active().expect("active fixture").claim();
    let reference = ReferenceState::active(ids.scope(), ids.holder());
    let command_id = command(case * 16 + seed);
    match case {
        0 => release_quiescent(seed, active, claim, reference, command_id),
        1 => reconciliation(seed, active, claim, reference, command_id),
        2 => expire(seed, active, reference, command_id),
        3 => holder_loss(seed, active, claim, reference, command_id),
        4 => clock_epoch(seed, active, reference, command_id),
        5 => clock_regression(seed, active, reference, command_id),
        6 => revoke(seed, active, claim, reference, command_id),
        7 => use_capability(seed, &ids, active, claim, reference, command_id),
        _ => release_reconciling(seed, active, claim, reference, command_id),
    }
}

fn release_quiescent(
    seed: u8,
    active: LeaseAggregate,
    claim: peritus_leases::LeaseClaim,
    before: ReferenceState,
    command_id: peritus_types::CommandId,
) {
    let quiescence = HolderQuiescenceEvidence::new(claim, evidence(seed));
    let command = ReleaseLease::new(command_id, claim, instant(20), Some(quiescence));
    finish(
        accepted(active.release(command)),
        &ExpectedTransition::new(
            Some(before),
            before.after_fence(true, instant(20), FenceCause::ReleasedWithoutQuiescence),
            command_id,
            LeaseTransitionKind::ReleasedAvailable,
            Box::new(ExpectedBinding::Release(command)),
            "release-quiescent",
        ),
        seed,
    );
}

fn expire(
    seed: u8,
    active: LeaseAggregate,
    before: ReferenceState,
    command_id: peritus_types::CommandId,
) {
    let command = ExpireLease::new(command_id, instant(60));
    finish(
        accepted(active.expire(command)),
        &ExpectedTransition::new(
            Some(before),
            before.after_fence(false, instant(60), FenceCause::Expired),
            command_id,
            LeaseTransitionKind::Expired,
            Box::new(ExpectedBinding::Expire(command)),
            "expire",
        ),
        seed,
    );
}

fn holder_loss(
    seed: u8,
    active: LeaseAggregate,
    claim: peritus_leases::LeaseClaim,
    before: ReferenceState,
    command_id: peritus_types::CommandId,
) {
    let command = FenceHolderLoss::new(
        command_id,
        instant(20),
        HolderLossEvidence::new(claim, evidence(seed)),
    );
    finish(
        accepted(active.fence_holder_loss(command)),
        &ExpectedTransition::new(
            Some(before),
            before.after_fence(false, instant(20), FenceCause::HolderLost),
            command_id,
            LeaseTransitionKind::HolderLost,
            Box::new(ExpectedBinding::HolderLoss(command)),
            "holder-loss",
        ),
        seed,
    );
}

fn clock_epoch(
    seed: u8,
    active: LeaseAggregate,
    before: ReferenceState,
    command_id: peritus_types::CommandId,
) {
    let observed_at = next_epoch(u64::from(seed));
    let command = FenceClockDiscontinuity::new(command_id, observed_at);
    finish(
        accepted(active.fence_clock_discontinuity(command)),
        &ExpectedTransition::new(
            Some(before),
            before.after_fence(false, observed_at, FenceCause::ClockDiscontinuity),
            command_id,
            LeaseTransitionKind::ClockDiscontinuity,
            Box::new(ExpectedBinding::ClockDiscontinuity(command)),
            "epoch-discontinuity",
        ),
        seed,
    );
}

fn clock_regression(
    seed: u8,
    active: LeaseAggregate,
    before: ReferenceState,
    command_id: peritus_types::CommandId,
) {
    let command = FenceClockDiscontinuity::new(command_id, instant(9));
    finish(
        accepted(active.fence_clock_discontinuity(command)),
        &ExpectedTransition::new(
            Some(before),
            before.after_fence(false, instant(9), FenceCause::ClockDiscontinuity),
            command_id,
            LeaseTransitionKind::ClockDiscontinuity,
            Box::new(ExpectedBinding::ClockDiscontinuity(command)),
            "clock-regression-fence",
        ),
        seed,
    );
}

fn revoke(
    seed: u8,
    active: LeaseAggregate,
    claim: peritus_leases::LeaseClaim,
    before: ReferenceState,
    command_id: peritus_types::CommandId,
) {
    let command = RevokeLease::new(command_id, claim, instant(20), evidence(seed));
    finish(
        accepted(active.revoke(command)),
        &ExpectedTransition::new(
            Some(before),
            before.after_fence(false, instant(20), FenceCause::Revoked),
            command_id,
            LeaseTransitionKind::Revoked,
            Box::new(ExpectedBinding::Revoke(command)),
            "revoke",
        ),
        seed,
    );
}

fn release_reconciling(
    seed: u8,
    active: LeaseAggregate,
    claim: peritus_leases::LeaseClaim,
    before: ReferenceState,
    command_id: peritus_types::CommandId,
) {
    let command = ReleaseLease::new(command_id, claim, instant(20), None);
    finish(
        accepted(active.release(command)),
        &ExpectedTransition::new(
            Some(before),
            before.after_fence(false, instant(20), FenceCause::ReleasedWithoutQuiescence),
            command_id,
            LeaseTransitionKind::ReleasedReconciling,
            Box::new(ExpectedBinding::Release(command)),
            "release-reconciling",
        ),
        seed,
    );
}

fn reconciliation(
    seed: u8,
    active: LeaseAggregate,
    claim: peritus_leases::LeaseClaim,
    before: ReferenceState,
    command_id: peritus_types::CommandId,
) {
    let release = ReleaseLease::new(command_id, claim, instant(20), None);
    let fenced_reference =
        before.after_fence(false, instant(20), FenceCause::ReleasedWithoutQuiescence);
    let fenced = finish(
        accepted(active.release(release)),
        &ExpectedTransition::new(
            Some(before),
            fenced_reference,
            command_id,
            LeaseTransitionKind::ReleasedReconciling,
            Box::new(ExpectedBinding::Release(release)),
            "reconcile-fence",
        ),
        seed,
    );
    let correlation = fenced.reconciliation().expect("reconciling").correlation();
    let disposition = match seed % 3 {
        0 => ReconciliationDisposition::SafeToAcquire {
            holder_quiescence: evidence(seed),
            resource_safety: evidence(seed + 16),
        },
        1 => ReconciliationDisposition::Dirty { evidence_id: evidence(seed) },
        _ => ReconciliationDisposition::Indeterminate { evidence_id: evidence(seed) },
    };
    let reconcile = ReconcileLease::new(
        command(seed + 224),
        instant(21),
        ReconciliationObservation::new(correlation, disposition),
    );
    let kind = if matches!(disposition, ReconciliationDisposition::SafeToAcquire { .. }) {
        LeaseTransitionKind::ReconciledAvailable
    } else {
        LeaseTransitionKind::ReconciledQuarantined
    };
    finish(
        accepted(fenced.reconcile(reconcile)),
        &ExpectedTransition::new(
            Some(fenced_reference),
            fenced_reference.after_reconcile(instant(21), disposition),
            command(seed + 224),
            kind,
            Box::new(ExpectedBinding::Reconcile(reconcile)),
            "reconcile-result",
        ),
        seed,
    );
}

fn use_capability(
    seed: u8,
    ids: &FixtureIds,
    active: LeaseAggregate,
    claim: peritus_leases::LeaseClaim,
    before: ReferenceState,
    command_id: peritus_types::CommandId,
) {
    let capability = crate::support::capability_use(
        ids,
        &crate::support::CapabilityUseFixture::new(
            ids.actor,
            ids.environment,
            ids.workspace,
            active.generation(),
            ids.resource,
            instant(20),
            action(seed),
        ),
    );
    let expected = ExpectedUseOutput::new(command_id, claim, instant(20), &capability);
    let logical_use =
        match active.authorize_use(UseLease::new(command_id, claim, instant(20), capability)) {
            LeaseUseOutcome::Accepted(value) => value,
            LeaseUseOutcome::Rejected(failure) => {
                panic!("seed {seed} accepted use rejected: {:?}", failure.error())
            }
        };
    expected.assert_matches(
        &logical_use,
        before,
        before.after_use(instant(20)),
        seed,
        "authorized-use",
    );
    let (lease, _consumed_capability) = logical_use.into_parts();
    let _aggregate = lease.into_next();
}

fn finish(transition: LeaseTransition, expected: &ExpectedTransition, seed: u8) -> LeaseAggregate {
    assert_transition(&transition, expected, seed);
    transition.into_next()
}
