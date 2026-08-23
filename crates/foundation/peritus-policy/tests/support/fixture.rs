//! Deterministic public-API fixtures shared by policy integration tests.

use peritus_policy::{
    ActorRole, ActorSelector, ApprovalRequirement, AuthorityBoundary, AuthorityCeiling,
    AuthorityInstant, CeilingGrant, EnvironmentSelector, IndependenceRequirement, IndependenceSet,
    OperationClass, OperationDescriptor, OperationRegistry, Permission, PermissionSelector,
    PermissionSet, PolicyDefinition, PolicyTier, RestrictionLayer, RestrictionRule, RiskClass,
    RiskSet, RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_test_support::DeterministicIdSource;
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, CommandId, EnvironmentId, Generation, HarnessId, PolicyId,
    ProviderProfileId, ResourceId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

pub struct FixtureIds {
    pub actor: ActorId,
    pub other_actor: ActorId,
    pub environment: EnvironmentId,
    pub other_environment: EnvironmentId,
    pub first_resource: ResourceId,
    pub second_resource: ResourceId,
    pub third_resource: ResourceId,
    pub policy: PolicyId,
    pub other_policy: PolicyId,
    pub acceptance: AcceptanceSpecId,
    pub other_acceptance: AcceptanceSpecId,
    pub harness: HarnessId,
    pub other_harness: HarnessId,
    pub workspace: WorkspaceId,
    pub other_workspace: WorkspaceId,
    pub provider: ProviderProfileId,
    pub other_provider: ProviderProfileId,
}

impl FixtureIds {
    pub fn new() -> Self {
        let mut source = DeterministicIdSource::new(*b"policy01");
        Self {
            actor: source.next(ActorId::new).expect("actor id"),
            other_actor: source.next(ActorId::new).expect("other actor id"),
            environment: source.next(EnvironmentId::new).expect("environment id"),
            other_environment: source.next(EnvironmentId::new).expect("other environment id"),
            first_resource: source.next(ResourceId::new).expect("first resource"),
            second_resource: source.next(ResourceId::new).expect("second resource"),
            third_resource: source.next(ResourceId::new).expect("third resource"),
            policy: source.next(PolicyId::new).expect("policy id"),
            other_policy: source.next(PolicyId::new).expect("other policy id"),
            acceptance: source.next(AcceptanceSpecId::new).expect("acceptance id"),
            other_acceptance: source.next(AcceptanceSpecId::new).expect("other acceptance id"),
            harness: source.next(HarnessId::new).expect("harness id"),
            other_harness: source.next(HarnessId::new).expect("other harness id"),
            workspace: source.next(WorkspaceId::new).expect("workspace id"),
            other_workspace: source.next(WorkspaceId::new).expect("other workspace id"),
            provider: source.next(ProviderProfileId::new).expect("provider id"),
            other_provider: source.next(ProviderProfileId::new).expect("other provider id"),
        }
    }

    pub const fn revision(&self) -> RevisionTuple {
        RevisionTuple::new(
            self.acceptance,
            self.harness,
            self.workspace,
            Generation::first(),
            RevisionNumber::first(),
            self.policy,
            self.provider,
        )
    }
}

#[derive(Clone, Copy)]
pub struct PermissionSpec {
    pub resource: ResourceId,
    pub name: &'static str,
}

pub struct ScopeInput {
    pub actor: ActorId,
    pub role: ActorRole,
    pub environment: EnvironmentId,
    pub permissions: Vec<PermissionSpec>,
    pub revision: RevisionTuple,
    pub validity: ValidityWindow,
    pub uses: UseLimit,
}

pub struct PolicyInput {
    pub actors: Vec<ActorId>,
    pub roles: Vec<ActorRole>,
    pub environments: Vec<EnvironmentId>,
    pub permissions: Vec<PermissionSpec>,
    pub revision: RevisionTuple,
    pub validity: ValidityWindow,
    pub uses: UseLimit,
    pub grants: Vec<CeilingGrant>,
    pub immutable_denies: Vec<RestrictionRule>,
    pub operations: OperationRegistry,
    pub layers: Vec<RestrictionLayer>,
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

pub fn instant(epoch: u64, tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::new(epoch).expect("positive epoch"), tick)
}

pub fn window(epoch: u64, start: u64, end: u64) -> ValidityWindow {
    ValidityWindow::new(instant(epoch, start), instant(epoch, end)).expect("valid window")
}

pub fn use_limit(remaining: Option<u64>) -> UseLimit {
    remaining.map_or_else(UseLimit::unlimited, |value| {
        UseLimit::limited(value).expect("positive use limit")
    })
}

pub fn permission(spec: PermissionSpec) -> Permission {
    Permission::new(
        spec.resource,
        peritus_types::CapabilityName::new(spec.name.to_owned()).expect("capability name"),
    )
}

pub fn permission_set(specs: Vec<PermissionSpec>) -> PermissionSet {
    let mut values: Vec<_> = specs.into_iter().map(permission).collect();
    values.sort_by(Permission::canonical_cmp);
    PermissionSet::new(values).expect("canonical permission set")
}

pub fn scope(input: ScopeInput) -> peritus_policy::CapabilityScope {
    peritus_policy::CapabilityScope::new(
        input.actor,
        input.role,
        input.environment,
        permission_set(input.permissions),
        input.revision,
        input.validity,
        input.uses,
    )
}

pub const fn any_selector(revision: RevisionTuple) -> ScopeSelector {
    ScopeSelector::new(
        ActorSelector::any_within_parent(),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        revision,
    )
}

pub fn permission_selector(
    revision: RevisionTuple,
    permissions: Vec<PermissionSpec>,
) -> ScopeSelector {
    ScopeSelector::new(
        ActorSelector::any_within_parent(),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::exact(permission_set(permissions)),
        revision,
    )
}

pub const fn grant(
    digest_byte: u8,
    selector: ScopeSelector,
    validity: ValidityWindow,
    uses: UseLimit,
) -> CeilingGrant {
    CeilingGrant::new(digest(digest_byte), selector, validity, uses)
}

pub const fn deny_rule(digest_byte: u8, selector: ScopeSelector) -> RestrictionRule {
    RestrictionRule::deny(digest(digest_byte), selector)
}

pub fn approval_requirement(
    tier: peritus_policy::AuthorityTier,
    mut roles: Vec<ActorRole>,
    independence: Vec<IndependenceRequirement>,
    validity: ValidityWindow,
) -> ApprovalRequirement {
    roles.sort_unstable();
    ApprovalRequirement::new(
        tier,
        roles,
        IndependenceSet::new(independence).expect("canonical independence"),
        validity,
    )
    .expect("approval requirement")
}

pub const fn approval_rule(
    digest_byte: u8,
    selector: ScopeSelector,
    requirement: ApprovalRequirement,
) -> RestrictionRule {
    RestrictionRule::require_approval(digest(digest_byte), selector, requirement)
}

pub fn layer(tier: PolicyTier, rules: Vec<RestrictionRule>) -> RestrictionLayer {
    RestrictionLayer::new(tier, rules).expect("restriction layer")
}

pub const fn mandatory_risk(operation: OperationClass) -> RiskClass {
    match operation {
        OperationClass::Inspection => RiskClass::Read,
        OperationClass::WorkspaceMutation => RiskClass::ScopedWrite,
        OperationClass::Execution => RiskClass::Execution,
        OperationClass::Network => RiskClass::Network,
        OperationClass::DependencyEnvironment => RiskClass::DependencyEnvironment,
        OperationClass::RepositoryHistoryMutation => RiskClass::RepositoryHistoryMutation,
        OperationClass::SecretUse => RiskClass::SecretUse,
        OperationClass::ExternalSideEffect | OperationClass::RawEffect => {
            RiskClass::ExternalSideEffect
        }
        OperationClass::Acceptance
        | OperationClass::Waiver
        | OperationClass::PolicyAmendment
        | OperationClass::HumanAuthority => RiskClass::PolicyAuthority,
        OperationClass::HarnessPromotion => RiskClass::HarnessPromotion,
    }
}

pub fn descriptor(name: &'static str, operation: OperationClass) -> OperationDescriptor {
    OperationDescriptor::new(
        peritus_types::CapabilityName::new(name.to_owned()).expect("operation name"),
        operation,
        RiskSet::new(vec![mandatory_risk(operation)]).expect("mandatory risk"),
    )
    .expect("consistent descriptor")
}

pub fn registry(mut descriptors: Vec<OperationDescriptor>) -> OperationRegistry {
    descriptors.sort_by(|left, right| left.name().canonical_cmp(right.name()));
    OperationRegistry::new(descriptors).expect("canonical operation registry")
}

pub fn policy(input: PolicyInput) -> PolicyDefinition {
    let mut actors = input.actors;
    actors.sort_by_key(|id| *id.as_bytes());
    let mut roles = input.roles;
    roles.sort_unstable();
    let mut environments = input.environments;
    environments.sort_by_key(|id| *id.as_bytes());
    let policy_id = input.revision.policy_id();
    let boundary = AuthorityBoundary::new(
        actors,
        roles,
        environments,
        permission_set(input.permissions),
        input.revision,
        input.validity,
        input.uses,
    )
    .expect("authority boundary");
    let ceiling = AuthorityCeiling::new(boundary, input.grants, input.immutable_denies)
        .expect("authority ceiling");
    PolicyDefinition::new(policy_id, ceiling, input.operations, input.layers)
        .expect("policy definition")
}
