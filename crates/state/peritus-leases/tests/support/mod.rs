#![allow(dead_code)]

pub mod lease_commit_claim;

use peritus_leases::{
    AcquireLease, ActiveLeaseView, LeaseAggregate, LeaseDuration, LeaseError, LeaseHolder,
    LeasePhase, LeaseScope, LeaseTransitionOutcome, MintLease, QuarantinedLeaseView,
    ReconciliationView, RetirementReason,
};
use peritus_policy::{
    ActorRole, ActorSelector, AuthorityBoundary, AuthorityCeiling, AuthorityInstant,
    AuthorityTimeState, AuthorizationRequest, CapabilityScope, CapabilityUseRequest,
    CapabilityUseTransition, CeilingGrant, EnvironmentSelector, OperationClass,
    OperationDescriptor, OperationRegistry, Permission, PermissionSelector, PermissionSet,
    PolicyDefinition, RiskClass, RiskSet, RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_test_support::DeterministicIdSource;
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, CapabilityName, CommandId, EnvironmentId, EvidenceId,
    Generation, HarnessId, PolicyId, ProviderProfileId, ResourceId, RevisionNumber, RevisionTuple,
    SessionId, Sha256Digest, WorkspaceId,
};

pub struct FixtureIds {
    pub workspace: WorkspaceId,
    pub other_workspace: WorkspaceId,
    pub resource: ResourceId,
    pub other_resource: ResourceId,
    pub environment: EnvironmentId,
    pub other_environment: EnvironmentId,
    pub actor: ActorId,
    pub other_actor: ActorId,
    pub session: SessionId,
    pub other_session: SessionId,
    pub policy: PolicyId,
    pub acceptance: AcceptanceSpecId,
    pub harness: HarnessId,
    pub provider: ProviderProfileId,
}

impl FixtureIds {
    pub fn new() -> Self {
        let mut source = DeterministicIdSource::new(*b"leases!!");
        Self {
            workspace: source.next(WorkspaceId::new).expect("workspace id"),
            other_workspace: source.next(WorkspaceId::new).expect("other workspace id"),
            resource: source.next(ResourceId::new).expect("resource id"),
            other_resource: source.next(ResourceId::new).expect("other resource id"),
            environment: source.next(EnvironmentId::new).expect("environment id"),
            other_environment: source.next(EnvironmentId::new).expect("other environment id"),
            actor: source.next(ActorId::new).expect("actor id"),
            other_actor: source.next(ActorId::new).expect("other actor id"),
            session: source.next(SessionId::new).expect("session id"),
            other_session: source.next(SessionId::new).expect("other session id"),
            policy: source.next(PolicyId::new).expect("policy id"),
            acceptance: source.next(AcceptanceSpecId::new).expect("acceptance id"),
            harness: source.next(HarnessId::new).expect("harness id"),
            provider: source.next(ProviderProfileId::new).expect("provider id"),
        }
    }

    pub const fn scope(&self) -> LeaseScope {
        LeaseScope::new(self.workspace, self.resource, self.environment)
    }

    pub const fn holder(&self) -> LeaseHolder {
        LeaseHolder::new(self.actor, self.session)
    }

    pub const fn other_holder(&self) -> LeaseHolder {
        LeaseHolder::new(self.other_actor, self.other_session)
    }

    pub const fn revision(&self, workspace: WorkspaceId, generation: Generation) -> RevisionTuple {
        RevisionTuple::new(
            self.acceptance,
            self.harness,
            workspace,
            generation,
            RevisionNumber::first(),
            self.policy,
            self.provider,
        )
    }
}

pub const fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}

pub fn next_epoch(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::new(2).expect("second epoch"), tick)
}

pub const fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::new([byte; 32])
}

pub fn command(byte: u8) -> CommandId {
    CommandId::new([byte; 16]).expect("command id")
}

pub fn action(byte: u8) -> ActionId {
    ActionId::new([byte; 16]).expect("action id")
}

pub fn evidence(byte: u8) -> EvidenceId {
    EvidenceId::new([byte; 16]).expect("evidence id")
}

pub fn mint(ids: &FixtureIds) -> LeaseAggregate {
    LeaseAggregate::mint(MintLease::new(command(1), ids.scope(), instant(10)))
        .expect("mint")
        .into_next()
}

pub fn active(ids: &FixtureIds) -> LeaseAggregate {
    accepted(mint(ids).acquire(AcquireLease::new(
        command(2),
        ids.holder(),
        LeaseDuration::new(50).expect("duration"),
        instant(10),
    )))
    .into_next()
}

#[derive(Debug, Eq, PartialEq)]
pub struct AggregateSnapshot {
    scope: LeaseScope,
    generation: Generation,
    version: RevisionNumber,
    authority_epoch: Generation,
    authority_tick: u64,
    phase: LeasePhase,
    active: Option<ActiveLeaseView>,
    reconciliation: Option<ReconciliationView>,
    quarantine: Option<QuarantinedLeaseView>,
    retirement: Option<RetirementReason>,
}

pub const fn snapshot(aggregate: &LeaseAggregate) -> AggregateSnapshot {
    AggregateSnapshot {
        scope: aggregate.scope(),
        generation: aggregate.generation(),
        version: aggregate.version(),
        authority_epoch: aggregate.authority_time().epoch(),
        authority_tick: aggregate.authority_time().greatest_tick_millis(),
        phase: aggregate.phase(),
        active: aggregate.active(),
        reconciliation: aggregate.reconciliation(),
        quarantine: aggregate.quarantine(),
        retirement: aggregate.retirement_reason(),
    }
}

pub fn recover_rejection(result: LeaseTransitionOutcome, expected: LeaseError) -> LeaseAggregate {
    match result {
        LeaseTransitionOutcome::Accepted(_) => panic!("command unexpectedly succeeded"),
        LeaseTransitionOutcome::Rejected(failure) => {
            assert_eq!(failure.error(), &expected);
            failure.into_aggregate()
        }
    }
}

pub fn accepted(outcome: LeaseTransitionOutcome) -> peritus_leases::LeaseTransition {
    match outcome {
        LeaseTransitionOutcome::Accepted(value) => value,
        LeaseTransitionOutcome::Rejected(failure) => {
            panic!("command rejected: {:?}", failure.error())
        }
    }
}

pub struct CapabilityUseFixture {
    pub actor: ActorId,
    pub environment: EnvironmentId,
    pub workspace: WorkspaceId,
    pub generation: Generation,
    pub resource: ResourceId,
    pub used_at: AuthorityInstant,
    pub action_id: ActionId,
}

impl CapabilityUseFixture {
    pub const fn new(
        actor: ActorId,
        environment: EnvironmentId,
        workspace: WorkspaceId,
        generation: Generation,
        resource: ResourceId,
        used_at: AuthorityInstant,
        action_id: ActionId,
    ) -> Self {
        Self { actor, environment, workspace, generation, resource, used_at, action_id }
    }
}

pub fn capability_use(ids: &FixtureIds, input: &CapabilityUseFixture) -> CapabilityUseTransition {
    let permission = || {
        Permission::new(
            input.resource,
            CapabilityName::new("workspace.mutate".to_owned()).expect("valid capability name"),
        )
    };
    let permissions = PermissionSet::new(vec![permission()]).expect("permission set");
    let validity = ValidityWindow::new(instant(0), instant(200)).expect("validity");
    let revision = ids.revision(input.workspace, input.generation);
    let scope = CapabilityScope::new(
        input.actor,
        ActorRole::Writer,
        input.environment,
        permissions,
        revision,
        validity,
        UseLimit::limited(2).expect("use limit"),
    );
    let boundary = AuthorityBoundary::new(
        vec![input.actor],
        vec![ActorRole::Writer],
        vec![input.environment],
        PermissionSet::new(vec![permission()]).expect("boundary permission set"),
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
            digest(10),
            selector,
            validity,
            UseLimit::limited(2).expect("grant use limit"),
        )],
        Vec::new(),
    )
    .expect("authority ceiling");
    let operations = OperationRegistry::new(vec![
        OperationDescriptor::new(
            CapabilityName::new("workspace.mutate".to_owned()).expect("operation capability name"),
            OperationClass::WorkspaceMutation,
            RiskSet::new(vec![RiskClass::ScopedWrite]).expect("risk set"),
        )
        .expect("valid operation descriptor"),
    ])
    .expect("operation registry");
    let policy =
        PolicyDefinition::new(ids.policy, ceiling, operations, Vec::new()).expect("policy");
    let request = AuthorizationRequest::new(scope);
    let decision = policy
        .evaluate(request, AuthorityTimeState::new(instant(0)), instant(1))
        .expect("policy evaluation");
    let (plan, challenge, denial) = decision.into_parts();
    assert!(challenge.is_none());
    assert!(denial.is_none());
    let plan = plan.expect("authorized issuance plan");
    let capability = plan.issue(command(90), digest(11)).into_capability();
    capability
        .try_use(
            CapabilityUseRequest::new(
                input.action_id,
                digest(12),
                permission(),
                input.actor,
                ActorRole::Writer,
                input.environment,
                revision,
                input.used_at,
            ),
            digest(13),
        )
        .expect("capability use")
}
