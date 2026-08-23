//! B1 policy and action digest wire contract tests.

use peritus_codec::{CodecErrorKind, CodecLimits, decode_message, encode_message};
use peritus_policy::{
    ActorRole, ActorSelector, AuthorityBoundary, AuthorityCeiling, AuthorityInstant, CeilingGrant,
    EnvironmentSelector, OperationClass, OperationDescriptor, OperationRegistry, Permission,
    PermissionSelector, PermissionSet, PolicyDefinition, PolicyTier, RestrictionLayer, RiskClass,
    RiskSet, RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_protocol::{
    ActionIntentDto, PolicyAmendmentDto, PolicyDefinitionDto, RestrictionLayerDto,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, CapabilityName, EnvironmentId, Generation, HarnessId,
    PolicyId, ProviderProfileId, ResourceId, RevisionNumber, RevisionTuple, Sha256Digest, TurnId,
    WorkspaceId,
};

const LIMITS: CodecLimits = CodecLimits::PRODUCTION;

fn fixture_id<T, E: core::fmt::Debug>(
    byte: u8,
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
) -> T {
    constructor([byte; 16]).expect("nonzero fixture id")
}

fn revision(policy_id: PolicyId) -> RevisionTuple {
    RevisionTuple::new(
        fixture_id(1, AcceptanceSpecId::new),
        fixture_id(2, HarnessId::new),
        fixture_id(3, WorkspaceId::new),
        Generation::new(4).expect("one based"),
        RevisionNumber::new(5).expect("one based"),
        policy_id,
        fixture_id(7, ProviderProfileId::new),
    )
}

fn validity() -> ValidityWindow {
    let epoch = Generation::new(1).expect("one based");
    ValidityWindow::new(AuthorityInstant::new(epoch, 10), AuthorityInstant::new(epoch, 100))
        .expect("ordered window")
}

fn definition() -> PolicyDefinition {
    let policy_id = fixture_id(10, PolicyId::new);
    let permission = Permission::new(
        fixture_id(11, ResourceId::new),
        CapabilityName::new("inspect".to_owned()).expect("name"),
    );
    let permissions = PermissionSet::new(vec![permission]).expect("canonical permission");
    let boundary = AuthorityBoundary::new(
        vec![fixture_id(12, ActorId::new)],
        vec![ActorRole::Writer],
        vec![fixture_id(13, EnvironmentId::new)],
        permissions,
        revision(policy_id),
        validity(),
        UseLimit::limited(9).expect("nonzero"),
    )
    .expect("boundary");
    let selector = ScopeSelector::new(
        ActorSelector::any_within_parent(),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        revision(policy_id),
    );
    let grant = CeilingGrant::new(
        Sha256Digest::new([20; 32]),
        selector,
        validity(),
        UseLimit::limited(8).expect("nonzero"),
    );
    let ceiling = AuthorityCeiling::new(boundary, vec![grant], vec![]).expect("ceiling");
    let operation = OperationDescriptor::new(
        CapabilityName::new("inspect".to_owned()).expect("name"),
        OperationClass::Inspection,
        RiskSet::new(vec![RiskClass::Read]).expect("risk"),
    )
    .expect("operation");
    let operations = OperationRegistry::new(vec![operation]).expect("registry");
    PolicyDefinition::new(
        policy_id,
        ceiling,
        operations,
        vec![RestrictionLayer::new(PolicyTier::Run, vec![]).expect("layer")],
    )
    .expect("definition")
}

#[test]
fn complete_policy_definition_roundtrips_through_checked_constructors() {
    let value = PolicyDefinitionDto::from(&definition());
    let bytes = encode_message(&value, LIMITS).expect("encode");
    let decoded: PolicyDefinitionDto = decode_message(&bytes, LIMITS).expect("decode");
    assert_eq!(decoded, value);
    assert_eq!(decoded.try_into_domain().expect("domain").policy_id(), value.policy_id);
}

#[test]
fn action_digest_binds_every_action_field() {
    let action = ActionIntentDto {
        action_id: fixture_id(30, ActionId::new),
        actor_id: fixture_id(31, ActorId::new),
        role: ActorRole::Writer,
        environment_id: fixture_id(32, EnvironmentId::new),
        resource_id: fixture_id(33, ResourceId::new),
        capability_name: CapabilityName::new("inspect".to_owned()).expect("name"),
        operation_class: OperationClass::Inspection,
        media_type: "application/json".to_owned(),
        payload: br#"{"path":"src/lib.rs"}"#.to_vec(),
    };
    let bytes = encode_message(&action, LIMITS).expect("encode");
    assert_eq!(decode_message::<ActionIntentDto>(&bytes, LIMITS).expect("decode"), action);
    let digest = action.digest(LIMITS).expect("digest");
    let command = action.propose_command(fixture_id(34, TurnId::new), LIMITS).expect("command");
    let peritus_kernel::KernelCommand::ProposeAction { digest: bound, .. } = command else {
        panic!("proposal")
    };
    assert_eq!(bound, digest);

    let mut mutations = Vec::new();
    let mut changed = action.clone();
    changed.action_id = fixture_id(35, ActionId::new);
    mutations.push(changed);
    let mut changed = action.clone();
    changed.actor_id = fixture_id(36, ActorId::new);
    mutations.push(changed);
    let mut changed = action.clone();
    changed.role = ActorRole::Reviewer;
    mutations.push(changed);
    let mut changed = action.clone();
    changed.environment_id = fixture_id(37, EnvironmentId::new);
    mutations.push(changed);
    let mut changed = action.clone();
    changed.resource_id = fixture_id(38, ResourceId::new);
    mutations.push(changed);
    let mut changed = action.clone();
    changed.capability_name = CapabilityName::new("execute".to_owned()).expect("name");
    mutations.push(changed);
    let mut changed = action.clone();
    changed.operation_class = OperationClass::Execution;
    mutations.push(changed);
    let mut changed = action.clone();
    changed.media_type = "text/plain".to_owned();
    mutations.push(changed);
    let mut changed = action;
    changed.payload.push(0);
    mutations.push(changed);
    for changed in mutations {
        assert_ne!(changed.digest(LIMITS).expect("changed digest"), digest);
    }
}

#[test]
fn amendment_digest_is_verified_before_domain_conversion() {
    let replacement = RestrictionLayer::new(PolicyTier::Run, vec![]).expect("layer");
    let value = PolicyAmendmentDto::new(
        fixture_id(40, PolicyId::new),
        fixture_id(41, PolicyId::new),
        PolicyTier::Run,
        RestrictionLayerDto::from(&replacement),
        LIMITS,
    )
    .expect("amendment");
    let bytes = encode_message(&value, LIMITS).expect("encode");
    let decoded: PolicyAmendmentDto = decode_message(&bytes, LIMITS).expect("decode");
    assert_eq!(decoded, value);
    decoded.try_into_domain(LIMITS).expect("checked proposal");

    let digest = value.amendment_digest;
    let changed = PolicyAmendmentDto::new(
        fixture_id(42, PolicyId::new),
        value.successor_policy_id,
        value.tier,
        value.replacement.clone(),
        LIMITS,
    )
    .expect("changed base");
    assert_ne!(changed.amendment_digest, digest);
    let changed = PolicyAmendmentDto::new(
        value.base_policy_id,
        fixture_id(43, PolicyId::new),
        value.tier,
        value.replacement.clone(),
        LIMITS,
    )
    .expect("changed successor");
    assert_ne!(changed.amendment_digest, digest);
    let changed = PolicyAmendmentDto::new(
        value.base_policy_id,
        value.successor_policy_id,
        PolicyTier::Session,
        value.replacement.clone(),
        LIMITS,
    )
    .expect("changed tier");
    assert_ne!(changed.amendment_digest, digest);
    let mut replacement = value.replacement.clone();
    replacement.tier = PolicyTier::Session;
    let changed = PolicyAmendmentDto::new(
        value.base_policy_id,
        value.successor_policy_id,
        value.tier,
        replacement,
        LIMITS,
    )
    .expect("changed replacement");
    assert_ne!(changed.amendment_digest, digest);

    let mut corrupt = bytes;
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert_eq!(
        decode_message::<PolicyAmendmentDto>(&corrupt, LIMITS).expect_err("digest mismatch").kind(),
        CodecErrorKind::InvalidDomainValue
    );
}
