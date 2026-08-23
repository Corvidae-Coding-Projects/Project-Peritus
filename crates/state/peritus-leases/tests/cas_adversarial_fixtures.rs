//! Reusable AC18 adversarial and crash-boundary fixture coverage.

mod support;

use peritus_leases::{
    AcquireLease, LeaseAggregate, LeaseCasPort, LeaseCasRequest, LeaseCasResolution, LeaseDuration,
    LeasePortFailure, ObservedLeaseState, ProtocolViolation,
};
use support::lease_commit_claim::{LeaseCasCall, LeaseCommitClaimFixture, ScriptedLeaseCas};
use support::{FixtureIds, accepted, command, instant};

fn mint_request(ids: &FixtureIds) -> LeaseCasRequest {
    let transition =
        LeaseAggregate::mint(peritus_leases::MintLease::new(command(1), ids.scope(), instant(10)))
            .expect("mint transition");
    LeaseCasRequest::from_transition(transition)
}

#[test]
fn forged_malformed_and_corrupt_observations_fail_closed() {
    let ids = FixtureIds::new();
    let cases = [
        (LeaseCommitClaimFixture::ForgedWorkspaceIdentity, ProtocolViolation::IdentityMismatch),
        (LeaseCommitClaimFixture::ForgedCommandIdentity, ProtocolViolation::IdentityMismatch),
        (LeaseCommitClaimFixture::MalformedObservation, ProtocolViolation::MalformedObservation),
        (LeaseCommitClaimFixture::CorruptSnapshot, ProtocolViolation::InvalidSnapshot),
    ];

    for (fixture, expected_violation) in cases {
        let request = mint_request(&ids);
        let mut port = ScriptedLeaseCas::for_fixture(fixture, &request, &ids, None);
        let observation = port.compare_and_swap(&request).expect("bounded observation");
        assert_eq!(
            request.resolve_observation(observation),
            LeaseCasResolution::ProtocolInvalid(expected_violation)
        );
        assert!(port.is_complete());
    }
}

#[test]
fn stale_snapshot_is_returned_only_as_unprivileged_conflict_evidence() {
    let ids = FixtureIds::new();
    let request = mint_request(&ids);
    let stale = support::active(&ids);
    let stale_scope = stale.scope();
    let stale_phase = stale.phase();
    let mut port = ScriptedLeaseCas::for_fixture(
        LeaseCommitClaimFixture::StaleSnapshot,
        &request,
        &ids,
        Some(stale),
    );

    let observation = port.compare_and_swap(&request).expect("bounded conflict");
    let LeaseCasResolution::Conflict(ObservedLeaseState::Present(observed)) =
        request.resolve_observation(observation)
    else {
        panic!("stale snapshot must remain conflict evidence");
    };
    assert_eq!(observed.scope(), stale_scope);
    assert_eq!(observed.phase(), stale_phase);
    assert!(port.is_complete());
}

#[test]
fn duplicate_claims_remain_separate_unprivileged_identity_observations() {
    let ids = FixtureIds::new();
    let request = mint_request(&ids);
    let mut port = ScriptedLeaseCas::for_fixture(
        LeaseCommitClaimFixture::DuplicateClaim,
        &request,
        &ids,
        None,
    );

    for _ in 0..2 {
        let observation = port.compare_and_swap(&request).expect("duplicate claim");
        let LeaseCasResolution::ClaimedApplied(claim) = request.resolve_observation(observation)
        else {
            panic!("exact duplicate identity must remain a bounded claim");
        };
        assert_eq!(claim.workspace_id(), request.workspace_id());
        assert_eq!(claim.command_id(), request.command_id());
    }
    assert_eq!(port.calls().len(), 2);
    assert!(port.is_complete());
}

#[test]
fn reused_command_with_a_different_plan_is_rejected() {
    let ids = FixtureIds::new();
    let first = accepted(support::mint(&ids).acquire(AcquireLease::new(
        command(2),
        ids.holder(),
        LeaseDuration::new(50).expect("duration"),
        instant(10),
    )));
    let second = accepted(support::mint(&ids).acquire(AcquireLease::new(
        command(2),
        ids.other_holder(),
        LeaseDuration::new(50).expect("duration"),
        instant(10),
    )));
    let request = LeaseCasRequest::from_transition(first);
    let second_record = second.record().duplicate();
    let second_plan = second.into_next();

    assert!(!request.authoritative_fields_match(
        request.workspace_id(),
        request.expected(),
        request.command_id(),
        &second_plan,
        &second_record,
    ));
    let mut port = ScriptedLeaseCas::for_fixture(
        LeaseCommitClaimFixture::ConflictingCommandReuse,
        &request,
        &ids,
        None,
    );
    let observation = port.compare_and_swap(&request).expect("bounded rejection");
    assert_eq!(
        request.resolve_observation(observation),
        LeaseCasResolution::ProtocolInvalid(ProtocolViolation::AuthoritativePlanMismatch)
    );
    assert!(port.is_complete());
}

#[test]
fn failure_before_commit_requires_reobservation_without_resolution() {
    let ids = FixtureIds::new();
    let request = mint_request(&ids);
    let mut port = ScriptedLeaseCas::for_fixture(
        LeaseCommitClaimFixture::FailureBeforeCommit,
        &request,
        &ids,
        None,
    );

    assert_eq!(port.compare_and_swap(&request), Err(LeasePortFailure::Unavailable));
    assert_eq!(
        port.calls(),
        [LeaseCasCall::Compare {
            workspace_id: request.workspace_id(),
            expected: request.expected(),
            command_id: request.command_id(),
        }]
    );
    assert!(port.is_complete());
}

#[test]
fn failure_after_commit_before_ack_resolves_the_exact_command() {
    let ids = FixtureIds::new();
    let request = mint_request(&ids);
    let mut port = ScriptedLeaseCas::for_fixture(
        LeaseCommitClaimFixture::FailureAfterCommitBeforeAck,
        &request,
        &ids,
        None,
    );

    assert_eq!(port.compare_and_swap(&request), Err(LeasePortFailure::Indeterminate));
    let resolved = port
        .resolve_command(request.workspace_id(), request.command_id())
        .expect("same-command resolution");
    assert!(matches!(request.resolve_observation(resolved), LeaseCasResolution::ClaimedApplied(_)));
    assert_eq!(
        port.calls(),
        [
            LeaseCasCall::Compare {
                workspace_id: request.workspace_id(),
                expected: request.expected(),
                command_id: request.command_id(),
            },
            LeaseCasCall::Resolve {
                workspace_id: request.workspace_id(),
                command_id: request.command_id(),
            },
        ]
    );
    assert!(port.is_complete());
}

#[test]
fn indeterminate_observation_resolves_to_a_definite_non_commit() {
    let ids = FixtureIds::new();
    let request = mint_request(&ids);
    let mut port = ScriptedLeaseCas::for_fixture(
        LeaseCommitClaimFixture::IndeterminateObservation,
        &request,
        &ids,
        None,
    );

    let first = port.compare_and_swap(&request).expect("bounded observation");
    assert_eq!(request.resolve_observation(first), LeaseCasResolution::Indeterminate);
    let resolved = port
        .resolve_command(request.workspace_id(), request.command_id())
        .expect("same-command resolution");
    assert_eq!(request.resolve_observation(resolved), LeaseCasResolution::DefinitelyNotApplied);
    assert!(port.is_complete());
}
