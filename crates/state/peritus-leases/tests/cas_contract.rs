//! Adversarial compare-and-swap observation contract tests.

mod support;

use peritus_leases::{
    AcquireLease, ExpireLease, LeaseCasExpectation, LeaseCasObservation, LeaseCasPort,
    LeaseCasRequest, LeaseCasResolution, LeaseDuration, LeasePortFailure, LeaseUseOutcome,
    ObservedLeaseState, ProtocolViolation, ReconcileLease, ReconciliationDisposition,
    ReconciliationObservation, RecoveryClass, RevokeLease, UseLease,
};
use peritus_types::{CommandId, WorkspaceId};
use std::collections::VecDeque;
use support::{
    CapabilityUseFixture, FixtureIds, accepted, action, capability_use, command, evidence, instant,
};

#[test]
fn exact_applied_claim_remains_an_unprivileged_observation() {
    let ids = FixtureIds::new();
    let mint = peritus_leases::LeaseAggregate::mint(peritus_leases::MintLease::new(
        command(1),
        ids.scope(),
        instant(10),
    ))
    .expect("mint");
    let request = LeaseCasRequest::from_transition(mint);
    assert_eq!(request.expected(), LeaseCasExpectation::Absent);
    assert!(request.authoritative_fields_match(
        request.workspace_id(),
        request.expected(),
        request.command_id(),
        request.planned(),
        request.record(),
    ));
    let resolution = request.resolve_observation(LeaseCasObservation::ClaimedApplied {
        workspace_id: request.workspace_id(),
        command_id: request.command_id(),
    });
    let LeaseCasResolution::ClaimedApplied(validated) = resolution else {
        panic!("exact identity claim must validate");
    };
    assert_eq!(validated.workspace_id(), request.workspace_id());
    assert_eq!(validated.command_id(), request.command_id());
}

#[test]
fn stale_authoritative_plan_and_identity_corruption_fail_closed() {
    let ids = FixtureIds::new();
    let available = support::mint(&ids);
    let planned = accepted(available.acquire(AcquireLease::new(
        command(2),
        ids.holder(),
        LeaseDuration::new(50).expect("duration"),
        instant(10),
    )));
    let stale = accepted(support::mint(&ids).acquire(AcquireLease::new(
        command(3),
        ids.other_holder(),
        LeaseDuration::new(50).expect("duration"),
        instant(10),
    )));
    let request = LeaseCasRequest::from_transition(planned);
    assert!(matches!(request.expected(), LeaseCasExpectation::Version(_)));

    let stale_record = stale.record().duplicate();
    let stale_plan = stale.into_next();
    assert!(!request.authoritative_fields_match(
        request.workspace_id(),
        request.expected(),
        request.command_id(),
        &stale_plan,
        &stale_record,
    ));
    assert!(matches!(
        request.resolve_observation(LeaseCasObservation::ClaimedApplied {
            workspace_id: ids.other_workspace,
            command_id: request.command_id(),
        }),
        LeaseCasResolution::ProtocolInvalid(ProtocolViolation::IdentityMismatch)
    ));

    assert!(matches!(
        request.resolve_observation(LeaseCasObservation::ClaimedApplied {
            workspace_id: request.workspace_id(),
            command_id: command(99),
        }),
        LeaseCasResolution::ProtocolInvalid(ProtocolViolation::IdentityMismatch)
    ));
}

#[test]
fn revoke_evidence_only_record_substitution_fails_full_plan_matching() {
    let ids = FixtureIds::new();
    let first_active = support::active(&ids);
    let first_claim = first_active.active().expect("active claim").claim();
    let first = accepted(first_active.revoke(RevokeLease::new(
        command(20),
        first_claim,
        instant(20),
        evidence(20),
    )));

    let second_active = support::active(&ids);
    let second_claim = second_active.active().expect("active claim").claim();
    let second = accepted(second_active.revoke(RevokeLease::new(
        command(20),
        second_claim,
        instant(20),
        evidence(21),
    )));

    let request = LeaseCasRequest::from_transition(first);
    let substituted_record = second.record().duplicate();
    let substituted_plan = second.into_next();
    assert_eq!(request.planned(), &substituted_plan);
    assert_ne!(request.record(), &substituted_record);

    assert!(!request.authoritative_fields_match(
        request.workspace_id(),
        request.expected(),
        request.command_id(),
        &substituted_plan,
        &substituted_record,
    ));
}

#[test]
fn reconciliation_evidence_only_record_substitution_fails_full_plan_matching() {
    let ids = FixtureIds::new();
    let first_fenced =
        accepted(support::active(&ids).expire(ExpireLease::new(command(30), instant(60))))
            .into_next();
    let first_correlation = first_fenced.reconciliation().expect("reconciliation").correlation();
    let first = accepted(first_fenced.reconcile(ReconcileLease::new(
        command(31),
        instant(61),
        ReconciliationObservation::new(
            first_correlation,
            ReconciliationDisposition::SafeToAcquire {
                holder_quiescence: evidence(30),
                resource_safety: evidence(31),
            },
        ),
    )));

    let second_fenced =
        accepted(support::active(&ids).expire(ExpireLease::new(command(30), instant(60))))
            .into_next();
    let second_correlation = second_fenced.reconciliation().expect("reconciliation").correlation();
    let second = accepted(second_fenced.reconcile(ReconcileLease::new(
        command(31),
        instant(61),
        ReconciliationObservation::new(
            second_correlation,
            ReconciliationDisposition::SafeToAcquire {
                holder_quiescence: evidence(32),
                resource_safety: evidence(33),
            },
        ),
    )));

    let request = LeaseCasRequest::from_transition(first);
    let substituted_record = second.record().duplicate();
    let substituted_plan = second.into_next();
    assert_eq!(request.planned(), &substituted_plan);
    assert_ne!(request.record(), &substituted_record);

    assert!(!request.authoritative_fields_match(
        request.workspace_id(),
        request.expected(),
        request.command_id(),
        &substituted_plan,
        &substituted_record,
    ));
}

#[test]
fn conflict_not_applied_and_indeterminate_have_distinct_recovery_paths() {
    let ids = FixtureIds::new();
    let mint = peritus_leases::LeaseAggregate::mint(peritus_leases::MintLease::new(
        command(1),
        ids.scope(),
        instant(10),
    ))
    .expect("mint");
    let request = LeaseCasRequest::from_transition(mint);

    assert!(matches!(
        request.resolve_observation(LeaseCasObservation::Conflict {
            workspace_id: request.workspace_id(),
            command_id: request.command_id(),
            observed: ObservedLeaseState::Absent,
        }),
        LeaseCasResolution::Conflict(ObservedLeaseState::Absent)
    ));
    assert!(matches!(
        request.resolve_observation(LeaseCasObservation::DefinitelyNotApplied {
            workspace_id: request.workspace_id(),
            command_id: request.command_id(),
        }),
        LeaseCasResolution::DefinitelyNotApplied
    ));
    assert!(matches!(
        request.resolve_observation(LeaseCasObservation::Indeterminate {
            workspace_id: request.workspace_id(),
            command_id: request.command_id(),
        }),
        LeaseCasResolution::Indeterminate
    ));
}

#[test]
fn every_port_failure_has_a_stable_code_and_recovery_class() {
    let cases = [
        (LeasePortFailure::Unavailable, "PERITUS-LEASE-PORT-001", RecoveryClass::Reobserve),
        (
            LeasePortFailure::Indeterminate,
            "PERITUS-LEASE-PORT-002",
            RecoveryClass::ResolveIndeterminate,
        ),
        (
            LeasePortFailure::ProtocolViolation(ProtocolViolation::IdentityMismatch),
            "PERITUS-LEASE-CAS-001",
            RecoveryClass::CallerCorrectable,
        ),
        (
            LeasePortFailure::ProtocolViolation(ProtocolViolation::AuthoritativePlanMismatch),
            "PERITUS-LEASE-CAS-002",
            RecoveryClass::CallerCorrectable,
        ),
        (
            LeasePortFailure::ProtocolViolation(ProtocolViolation::InvalidSnapshot),
            "PERITUS-LEASE-CAS-003",
            RecoveryClass::Terminal,
        ),
        (
            LeasePortFailure::ProtocolViolation(ProtocolViolation::MalformedObservation),
            "PERITUS-LEASE-CAS-004",
            RecoveryClass::CallerCorrectable,
        ),
    ];
    for (failure, code, recovery) in cases {
        assert_eq!(failure.code(), code);
        assert_eq!(failure.recovery(), recovery);
    }
}

struct ScriptedCas {
    observations: VecDeque<Result<LeaseCasObservation, LeasePortFailure>>,
}

struct BorrowingClaimCas;

impl LeaseCasPort for BorrowingClaimCas {
    fn compare_and_swap(
        &mut self,
        request: &LeaseCasRequest,
    ) -> Result<LeaseCasObservation, LeasePortFailure> {
        Ok(LeaseCasObservation::ClaimedApplied {
            workspace_id: request.workspace_id(),
            command_id: request.command_id(),
        })
    }

    fn resolve_command(
        &mut self,
        workspace_id: WorkspaceId,
        command_id: CommandId,
    ) -> Result<LeaseCasObservation, LeasePortFailure> {
        Ok(LeaseCasObservation::DefinitelyNotApplied { workspace_id, command_id })
    }
}

#[test]
fn borrowed_port_can_claim_an_authorize_use_plan_without_reconstructing_it() {
    let ids = FixtureIds::new();
    let active = support::active(&ids);
    let claim = active.active().expect("active claim").claim();
    let policy_use = capability_use(
        &ids,
        &CapabilityUseFixture::new(
            ids.actor,
            ids.environment,
            ids.workspace,
            active.generation(),
            ids.resource,
            instant(20),
            action(80),
        ),
    );
    let logical_use =
        match active.authorize_use(UseLease::new(command(80), claim, instant(20), policy_use)) {
            LeaseUseOutcome::Accepted(value) => value,
            LeaseUseOutcome::Rejected(failure) => {
                panic!("authorize-use plan rejected: {:?}", failure.error())
            }
        };
    let (lease_transition, _consumed_capability_use) = logical_use.into_parts();
    let request = LeaseCasRequest::from_transition(lease_transition);
    let mut port = BorrowingClaimCas;
    let observation = port.compare_and_swap(&request).expect("bounded applied claim");
    let resolution = request.resolve_observation(observation);

    assert!(matches!(resolution, LeaseCasResolution::ClaimedApplied(_)));
    assert_eq!(request.record().command_id(), command(80));
}

impl LeaseCasPort for ScriptedCas {
    fn compare_and_swap(
        &mut self,
        _request: &LeaseCasRequest,
    ) -> Result<LeaseCasObservation, LeasePortFailure> {
        self.observations.pop_front().expect("scripted CAS observation")
    }

    fn resolve_command(
        &mut self,
        _workspace_id: WorkspaceId,
        _command_id: CommandId,
    ) -> Result<LeaseCasObservation, LeasePortFailure> {
        self.observations.pop_front().expect("scripted resolution observation")
    }
}

#[test]
fn indeterminate_port_outcome_is_resolved_under_the_same_command_id() {
    let ids = FixtureIds::new();
    let mint = peritus_leases::LeaseAggregate::mint(peritus_leases::MintLease::new(
        command(1),
        ids.scope(),
        instant(10),
    ))
    .expect("mint");
    let request = LeaseCasRequest::from_transition(mint);
    let mut port = ScriptedCas {
        observations: VecDeque::from([
            Ok(LeaseCasObservation::Indeterminate {
                workspace_id: request.workspace_id(),
                command_id: request.command_id(),
            }),
            Ok(LeaseCasObservation::DefinitelyNotApplied {
                workspace_id: request.workspace_id(),
                command_id: request.command_id(),
            }),
        ]),
    };
    let first = port.compare_and_swap(&request).expect("bounded observation");
    assert!(matches!(request.resolve_observation(first), LeaseCasResolution::Indeterminate));
    let resolved = port
        .resolve_command(request.workspace_id(), request.command_id())
        .expect("bounded resolution");
    assert!(matches!(
        request.resolve_observation(resolved),
        LeaseCasResolution::DefinitelyNotApplied
    ));
}
