//! Illegal lifecycle edges from terminal retired aggregates.

use super::*;
use crate::state::ActiveLease;
use crate::{
    AcquireLease, ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, HolderLossEvidence,
    LeaseDuration, LeaseHolder, LeaseScope, LeaseUseOutcome, ReconcileLease,
    ReconciliationCorrelation, ReconciliationDisposition, ReconciliationObservation, ReleaseLease,
    RenewLease, RevokeLease, UseLease,
};
use peritus_policy::{
    ActorRole, ActorSelector, AuthorityBoundary, AuthorityCeiling, AuthorityInstant,
    AuthorityTimeState, AuthorizationRequest, CapabilityScope, CapabilityUseRequest,
    CapabilityUseTransition, CeilingGrant, EnvironmentSelector, OperationClass,
    OperationDescriptor, OperationRegistry, Permission, PermissionSelector, PermissionSet,
    PolicyDefinition, RiskClass, RiskSet, RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, CapabilityName, CommandId, EnvironmentId, EvidenceId,
    Generation, HarnessId, PolicyId, ProviderProfileId, ResourceId, RevisionNumber, RevisionTuple,
    SessionId, Sha256Digest, WorkspaceId,
};

const fn identifier(byte: u8) -> [u8; 16] { [byte; 16] }

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

fn active(generation: Generation) -> LeaseAggregate {
    LeaseAggregate::from_parts(
        scope(),
        generation,
        RevisionNumber::new(2).expect("version"),
        AuthorityTimeState::new(instant(10)),
        LeaseState::Active(ActiveLease {
            holder: holder(),
            claim_version: RevisionNumber::first(),
            issued_at: instant(10),
            expires_at: instant(20),
        }),
    )
}

fn accepted(outcome: LeaseTransitionOutcome) -> LeaseTransition {
    match outcome {
        LeaseTransitionOutcome::Accepted(transition) => transition,
        LeaseTransitionOutcome::Rejected(failure) => {
            panic!("retirement rejected: {:?}", failure.error())
        }
    }
}

fn retired() -> LeaseAggregate {
    accepted(
        active(Generation::new(u64::MAX).expect("maximum generation"))
            .expire(ExpireLease::new(command(99), instant(20))),
    )
    .into_next()
}

fn claim() -> LeaseClaim {
    active(Generation::first()).active().expect("claim donor").claim()
}

const fn digest(byte: u8) -> Sha256Digest { Sha256Digest::new([byte; 32]) }

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(identifier(11)).expect("acceptance"),
        HarnessId::new(identifier(12)).expect("harness"),
        scope().workspace_id(),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(identifier(13)).expect("policy"),
        ProviderProfileId::new(identifier(14)).expect("provider"),
    )
}

fn permission() -> Permission {
    Permission::new(
        scope().resource_id(),
        CapabilityName::new("workspace.mutate".to_owned()).expect("capability name"),
    )
}

fn capability_use() -> CapabilityUseTransition {
    let actor = holder().actor_id();
    let environment = scope().environment_id();
    let exact_permission = permission();
    let permissions = PermissionSet::new(vec![permission()]).expect("permissions");
    let validity = ValidityWindow::new(instant(0), instant(200)).expect("validity");
    let revision = revision();
    let capability_scope = CapabilityScope::new(
        actor,
        ActorRole::Writer,
        environment,
        permissions,
        revision,
        validity,
        UseLimit::limited(2).expect("scope use limit"),
    );
    let boundary = AuthorityBoundary::new(
        vec![actor],
        vec![ActorRole::Writer],
        vec![environment],
        PermissionSet::new(vec![permission()]).expect("boundary permissions"),
        revision,
        validity,
        UseLimit::limited(2).expect("boundary use limit"),
    )
    .expect("authority boundary");
    let selector = ScopeSelector::new(
        ActorSelector::any_within_parent(),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        revision,
    );
    let ceiling = AuthorityCeiling::new(
        boundary,
        vec![CeilingGrant::new(
            digest(15),
            selector,
            validity,
            UseLimit::limited(2).expect("grant use limit"),
        )],
        Vec::new(),
    )
    .expect("authority ceiling");
    let operations = OperationRegistry::new(vec![
        OperationDescriptor::new(
            CapabilityName::new("workspace.mutate".to_owned()).expect("operation capability"),
            OperationClass::WorkspaceMutation,
            RiskSet::new(vec![RiskClass::ScopedWrite]).expect("risk set"),
        )
        .expect("operation descriptor"),
    ])
    .expect("operation registry");
    let policy = PolicyDefinition::new(revision.policy_id(), ceiling, operations, Vec::new())
        .expect("policy");
    let decision = policy
        .evaluate(
            AuthorizationRequest::new(capability_scope),
            AuthorityTimeState::new(instant(0)),
            instant(1),
        )
        .expect("policy evaluation");
    let (plan, challenge, denial) = decision.into_parts();
    assert!(challenge.is_none());
    assert!(denial.is_none());
    let capability = plan
        .expect("authorized issuance plan")
        .issue(command(109), digest(16))
        .into_capability();
    capability
        .try_use(
            CapabilityUseRequest::new(
                ActionId::new(identifier(17)).expect("action"),
                digest(18),
                exact_permission,
                actor,
                ActorRole::Writer,
                environment,
                revision,
                instant(10),
            ),
            digest(19),
        )
        .expect("capability use")
}

fn assert_rejection(result: LeaseTransitionOutcome, expected: LeaseError) {
    let failure = match result {
        LeaseTransitionOutcome::Accepted(_) => panic!("retired command succeeded"),
        LeaseTransitionOutcome::Rejected(failure) => failure,
    };
    assert_eq!(failure.error(), &expected);
    let aggregate = failure.into_aggregate();
    assert_eq!(aggregate.phase(), LeasePhase::Retired);
    assert_eq!(aggregate.generation().get(), u64::MAX);
    assert_eq!(aggregate.version().get(), 3);
    assert_eq!(aggregate.retirement_reason(), Some(RetirementReason::GenerationExhausted));
}

fn assert_use_rejection(result: LeaseUseOutcome, expected: LeaseError) {
    let failure = match result {
        LeaseUseOutcome::Accepted(_) => panic!("retired policy use succeeded"),
        LeaseUseOutcome::Rejected(failure) => failure,
    };
    assert_eq!(failure.error(), &expected);
    let (lease, _) = failure.into_parts();
    assert_rejection(LeaseTransitionOutcome::Rejected(lease), expected);
}

#[test]
fn every_retired_phase_command_edge_rejects_without_state_change() {
    let claim = claim();
    let expected_active = LeaseError::IllegalPhase {
        expected: LeasePhase::Active,
        actual: LeasePhase::Retired,
    };
    assert_rejection(
        retired().acquire(AcquireLease::new(
            command(100),
            holder(),
            LeaseDuration::new(10).expect("duration"),
            instant(20),
        )),
        LeaseError::IllegalPhase {
            expected: LeasePhase::Available,
            actual: LeasePhase::Retired,
        },
    );
    assert_rejection(
        retired().renew(RenewLease::new(
            command(101), claim, LeaseDuration::new(10).expect("duration"), instant(20),
        )),
        expected_active,
    );
    assert_use_rejection(
        retired().authorize_use(UseLease::new(command(108), claim, instant(10), capability_use())),
        expected_active,
    );
    assert_rejection(
        retired().release(ReleaseLease::new(command(102), claim, instant(20), None)),
        expected_active,
    );
    assert_rejection(
        retired().expire(ExpireLease::new(command(103), instant(20))),
        expected_active,
    );
    assert_rejection(
        retired().fence_holder_loss(FenceHolderLoss::new(
            command(104),
            instant(20),
            HolderLossEvidence::new(
                claim,
                EvidenceId::new(identifier(8)).expect("evidence"),
            ),
        )),
        expected_active,
    );
    assert_rejection(
        retired().fence_clock_discontinuity(FenceClockDiscontinuity::new(
            command(105),
            AuthorityInstant::new(Generation::new(2).expect("epoch"), 1),
        )),
        expected_active,
    );
    assert_rejection(
        retired().revoke(RevokeLease::new(
            command(106),
            claim,
            instant(20),
            EvidenceId::new(identifier(9)).expect("evidence"),
        )),
        expected_active,
    );
    let correlation = ReconciliationCorrelation::new(scope(), Generation::first(), holder());
    assert_rejection(
        retired().reconcile(ReconcileLease::new(
            command(107),
            instant(20),
            ReconciliationObservation::new(
                correlation,
                ReconciliationDisposition::Dirty {
                    evidence_id: EvidenceId::new(identifier(10)).expect("evidence"),
                },
            ),
        )),
        LeaseError::IllegalPhase {
            expected: LeasePhase::Reconciling,
            actual: LeasePhase::Retired,
        },
    );
}
