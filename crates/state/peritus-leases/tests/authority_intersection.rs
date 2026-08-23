//! Exact policy-capability and current-lease intersection tests.

mod support;

use peritus_leases::{
    AcquireLease, ExpireLease, LeaseAggregate, LeaseDuration, LeaseError, LeaseUseOutcome,
    PolicyIntersectionDimension, ReconcileLease, ReconciliationDisposition,
    ReconciliationObservation, UseLease,
};
use peritus_policy::CapabilityUseTransition;
use peritus_types::Generation;
use support::{
    CapabilityUseFixture, FixtureIds, accepted, action, capability_use, command, evidence, instant,
};

#[test]
fn exact_policy_use_and_current_claim_produce_one_move_only_logical_use() {
    let ids = FixtureIds::new();
    let active = support::active(&ids);
    let claim = active.active().expect("active").claim();
    let policy_use = capability_use(
        &ids,
        &CapabilityUseFixture::new(
            ids.actor,
            ids.environment,
            ids.workspace,
            active.generation(),
            ids.resource,
            instant(20),
            action(1),
        ),
    );
    let before_version = active.version();
    let logical_use =
        match active.authorize_use(UseLease::new(command(20), claim, instant(20), policy_use)) {
            LeaseUseOutcome::Accepted(logical_use) => logical_use,
            LeaseUseOutcome::Rejected(failure) => {
                panic!("exact intersection rejected: {:?}", failure.error())
            }
        };
    assert_eq!(logical_use.action_id(), action(1));
    assert_eq!(logical_use.effective_expires_at(), claim.expires_at());
    assert_eq!(logical_use.lease_transition().next().authority_time().greatest_tick_millis(), 20);
    assert_eq!(logical_use.lease_transition().next().version().get(), before_version.get() + 1);
}

fn scope_mismatch_cases(
    ids: &FixtureIds,
    active: &LeaseAggregate,
) -> [(CapabilityUseTransition, PolicyIntersectionDimension); 5] {
    [
        (
            capability_use(
                ids,
                &CapabilityUseFixture::new(
                    ids.other_actor,
                    ids.environment,
                    ids.workspace,
                    active.generation(),
                    ids.resource,
                    instant(20),
                    action(2),
                ),
            ),
            PolicyIntersectionDimension::Actor,
        ),
        (
            capability_use(
                ids,
                &CapabilityUseFixture::new(
                    ids.actor,
                    ids.other_environment,
                    ids.workspace,
                    active.generation(),
                    ids.resource,
                    instant(20),
                    action(3),
                ),
            ),
            PolicyIntersectionDimension::Environment,
        ),
        (
            capability_use(
                ids,
                &CapabilityUseFixture::new(
                    ids.actor,
                    ids.environment,
                    ids.other_workspace,
                    active.generation(),
                    ids.resource,
                    instant(20),
                    action(4),
                ),
            ),
            PolicyIntersectionDimension::Workspace,
        ),
        (
            capability_use(
                ids,
                &CapabilityUseFixture::new(
                    ids.actor,
                    ids.environment,
                    ids.workspace,
                    Generation::new(active.generation().get() + 1).expect("next generation"),
                    ids.resource,
                    instant(20),
                    action(5),
                ),
            ),
            PolicyIntersectionDimension::Generation,
        ),
        (
            capability_use(
                ids,
                &CapabilityUseFixture::new(
                    ids.actor,
                    ids.environment,
                    ids.workspace,
                    active.generation(),
                    ids.other_resource,
                    instant(20),
                    action(6),
                ),
            ),
            PolicyIntersectionDimension::ResourcePermission,
        ),
    ]
}

#[test]
fn every_exact_scope_dimension_fails_closed() {
    let ids = FixtureIds::new();
    let mut active = support::active(&ids);
    let claim = active.active().expect("active").claim();
    let cases = scope_mismatch_cases(&ids, &active);

    for (index, (policy_use, dimension)) in cases.into_iter().enumerate() {
        let result = active.authorize_use(UseLease::new(
            command(u8::try_from(30 + index).expect("small command")),
            claim,
            instant(20),
            policy_use,
        ));
        let failure = match result {
            LeaseUseOutcome::Accepted(_) => {
                panic!("dimension {dimension:?} unexpectedly authorized")
            }
            LeaseUseOutcome::Rejected(failure) => failure,
        };
        assert_eq!(
            failure.error(),
            &LeaseError::PolicyIntersectionMismatch(dimension),
            "dimension {dimension:?}"
        );
        let (lease_failure, _rejected_command) = failure.into_parts();
        active = lease_failure.into_aggregate();
        assert_eq!(active.version().get(), 2, "rejection preserves input state");
    }
}

#[test]
fn old_generation_policy_use_is_rejected_after_fence_and_reacquire() {
    let ids = FixtureIds::new();
    let active = support::active(&ids);
    let old_generation = active.generation();
    let fenced = accepted(active.expire(ExpireLease::new(command(40), instant(60)))).into_next();
    let correlation = fenced.reconciliation().expect("reconciliation").correlation();
    let available = accepted(fenced.reconcile(ReconcileLease::new(
        command(41),
        instant(61),
        ReconciliationObservation::new(
            correlation,
            ReconciliationDisposition::SafeToAcquire {
                holder_quiescence: evidence(7),
                resource_safety: evidence(8),
            },
        ),
    )))
    .into_next();
    let active = accepted(available.acquire(AcquireLease::new(
        command(42),
        ids.holder(),
        LeaseDuration::new(20).expect("duration"),
        instant(62),
    )))
    .into_next();
    let claim = active.active().expect("active").claim();
    let stale_policy_use = capability_use(
        &ids,
        &CapabilityUseFixture::new(
            ids.actor,
            ids.environment,
            ids.workspace,
            old_generation,
            ids.resource,
            instant(63),
            action(9),
        ),
    );
    let failure = match active.authorize_use(UseLease::new(
        command(43),
        claim,
        instant(63),
        stale_policy_use,
    )) {
        LeaseUseOutcome::Accepted(_) => {
            panic!("stale generation unexpectedly authorized")
        }
        LeaseUseOutcome::Rejected(failure) => failure,
    };
    assert_eq!(
        failure.error(),
        &LeaseError::PolicyIntersectionMismatch(PolicyIntersectionDimension::Generation)
    );
}

#[test]
fn capability_use_observation_must_equal_lease_observation() {
    let ids = FixtureIds::new();
    let active = support::active(&ids);
    let claim = active.active().expect("active").claim();
    let policy_use = capability_use(
        &ids,
        &CapabilityUseFixture::new(
            ids.actor,
            ids.environment,
            ids.workspace,
            active.generation(),
            ids.resource,
            instant(20),
            action(10),
        ),
    );
    let failure =
        match active.authorize_use(UseLease::new(command(44), claim, instant(21), policy_use)) {
            LeaseUseOutcome::Accepted(_) => {
                panic!("mismatched observation unexpectedly authorized")
            }
            LeaseUseOutcome::Rejected(failure) => failure,
        };
    assert_eq!(
        failure.error(),
        &LeaseError::PolicyIntersectionMismatch(PolicyIntersectionDimension::ClockEpoch)
    );
}
