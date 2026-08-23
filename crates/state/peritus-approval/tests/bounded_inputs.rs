//! Exact maximum, one-over, empty, duplicate, and canonical-order input checks.

mod support;

use peritus_approval::{
    AmendmentIdentity, ApprovalError, ApprovalKeyId, ApprovalPublicKey, ApproverCredential,
    CanonicalCollection, CredentialDimension, CredentialRegistrySnapshot, CredentialStatus,
    MAX_APPROVAL_PERMISSIONS, MAX_CREDENTIAL_APPROVAL_ROLES, MAX_CREDENTIAL_REGISTRY_ENTRIES,
    MAX_INDEPENDENCE_REQUIREMENTS, MAX_PRODUCING_PARTICIPANTS, MAX_REVIEW_PARTICIPANTS,
    MAX_RISK_CLASSES, ParticipantSet, ScopeDimension,
};
use peritus_policy::{
    ActorRole, AuthorityTier, IndependenceRequirement, IndependenceSet, PolicyTier, RiskClass,
    RiskSet,
};
use peritus_types::{
    ActorId, EnvironmentId, Generation, PolicyId, RevisionNumber, Sha256Digest, WorkspaceId,
};

fn actor(index: usize) -> ActorId {
    let mut bytes = [0_u8; 16];
    bytes[0] = 0x70;
    bytes[8..].copy_from_slice(&u64::try_from(index).expect("bounded actor index").to_be_bytes());
    ActorId::new(bytes).expect("nonzero actor")
}

fn actors(count: usize) -> Vec<ActorId> {
    (0..count).map(actor).collect()
}

fn all_roles() -> Vec<ActorRole> {
    vec![
        ActorRole::Writer,
        ActorRole::Fixer,
        ActorRole::Reviewer,
        ActorRole::Evaluator,
        ActorRole::GateRunner,
        ActorRole::Orchestrator,
        ActorRole::EvolutionAgent,
        ActorRole::HumanAuthority,
        ActorRole::DaemonService,
        ActorRole::ProviderToolWorker,
        ActorRole::Plugin,
    ]
}

fn structural_credential(index: u32) -> ApproverCredential {
    let mut bytes = [0_u8; 32];
    bytes[..4].copy_from_slice(&index.to_be_bytes());
    let public_key = ApprovalPublicKey::new(bytes);
    let key_id = ApprovalKeyId::compute(public_key).expect("fixed-size key ID preimage");
    ApproverCredential::new(
        key_id,
        public_key,
        actor(1),
        ActorRole::HumanAuthority,
        EnvironmentId::new([0x71; 16]).expect("environment"),
        WorkspaceId::new([0x72; 16]).expect("workspace"),
        AuthorityTier::Organization,
        vec![ActorRole::HumanAuthority],
        support::window(0, 100),
        Generation::first(),
        CredentialStatus::Enabled,
    )
    .expect("structurally valid credential")
}

fn registry_entries(count: usize) -> Vec<ApproverCredential> {
    let mut entries: Vec<_> = (0..count)
        .map(|index| structural_credential(u32::try_from(index).expect("bounded registry index")))
        .collect();
    entries.sort_by_key(ApproverCredential::key_id);
    entries
}

#[test]
fn permission_and_participant_limits_reject_exactly_one_over() {
    assert!(support::request_result(MAX_APPROVAL_PERMISSIONS, Vec::new()).is_ok());
    assert_eq!(
        support::request_result(MAX_APPROVAL_PERMISSIONS + 1, Vec::new())
            .expect_err("one-over request permissions"),
        ApprovalError::CollectionTooLarge(CanonicalCollection::Permissions),
    );

    assert!(ParticipantSet::producing(actors(MAX_PRODUCING_PARTICIPANTS)).is_ok());
    assert_eq!(
        ParticipantSet::producing(actors(MAX_PRODUCING_PARTICIPANTS + 1))
            .expect_err("one-over producing participants"),
        ApprovalError::CollectionTooLarge(CanonicalCollection::ProducingParticipants),
    );
    assert!(ParticipantSet::review(actors(MAX_REVIEW_PARTICIPANTS)).is_ok());
    assert_eq!(
        ParticipantSet::review(actors(MAX_REVIEW_PARTICIPANTS + 1))
            .expect_err("one-over review participants"),
        ApprovalError::CollectionTooLarge(CanonicalCollection::ReviewParticipants),
    );
}

#[test]
fn participant_sets_reject_duplicates_and_noncanonical_order() {
    assert_eq!(
        ParticipantSet::producing(vec![actor(1), actor(1)]).expect_err("duplicate actor"),
        ApprovalError::DuplicateCanonicalValue(CanonicalCollection::ProducingParticipants),
    );
    assert_eq!(
        ParticipantSet::review(vec![actor(2), actor(1)]).expect_err("descending actors"),
        ApprovalError::NonCanonicalOrder(CanonicalCollection::ReviewParticipants),
    );
}

#[test]
fn credential_role_limit_and_canonicality_are_exact() {
    assert_eq!(all_roles().len(), MAX_CREDENTIAL_APPROVAL_ROLES);
    let public_key = ApprovalPublicKey::new([0x31; 32]);
    let key_id = ApprovalKeyId::compute(public_key).expect("key ID");
    let construct = |roles| {
        ApproverCredential::new(
            key_id,
            public_key,
            actor(3),
            ActorRole::HumanAuthority,
            EnvironmentId::new([0x73; 16]).expect("environment"),
            WorkspaceId::new([0x74; 16]).expect("workspace"),
            AuthorityTier::System,
            roles,
            support::window(0, 100),
            Generation::first(),
            CredentialStatus::Enabled,
        )
    };
    assert!(construct(all_roles()).is_ok());
    let mut one_over = all_roles();
    one_over.push(ActorRole::Plugin);
    assert_eq!(
        construct(one_over).expect_err("one-over credential roles"),
        ApprovalError::CollectionTooLarge(CanonicalCollection::CredentialApprovalRoles),
    );
    assert_eq!(
        construct(Vec::new()).expect_err("empty credential roles"),
        ApprovalError::EmptyCollection(CanonicalCollection::CredentialApprovalRoles),
    );
    assert_eq!(
        construct(vec![ActorRole::Writer, ActorRole::Writer]).expect_err("duplicate role"),
        ApprovalError::DuplicateCanonicalValue(CanonicalCollection::CredentialApprovalRoles),
    );
    assert_eq!(
        construct(vec![ActorRole::Fixer, ActorRole::Writer]).expect_err("descending roles"),
        ApprovalError::NonCanonicalOrder(CanonicalCollection::CredentialApprovalRoles),
    );
}

#[test]
fn credential_constructor_rejects_key_and_principal_mismatch() {
    let public_key = ApprovalPublicKey::new([0x32; 32]);
    let other_key_id = ApprovalKeyId::compute(ApprovalPublicKey::new([0x33; 32]))
        .expect("other fixed-size key ID");
    let construct = |key_id, principal_role| {
        ApproverCredential::new(
            key_id,
            public_key,
            actor(4),
            principal_role,
            EnvironmentId::new([0x77; 16]).expect("environment"),
            WorkspaceId::new([0x78; 16]).expect("workspace"),
            AuthorityTier::System,
            vec![ActorRole::HumanAuthority],
            support::window(0, 100),
            Generation::first(),
            CredentialStatus::Enabled,
        )
    };
    assert_eq!(
        construct(other_key_id, ActorRole::HumanAuthority).expect_err("key ID mismatch"),
        ApprovalError::CredentialMismatch(CredentialDimension::KeyId),
    );
    let matching_key_id = ApprovalKeyId::compute(public_key).expect("matching key ID");
    assert_eq!(
        construct(matching_key_id, ActorRole::Writer).expect_err("non-human principal"),
        ApprovalError::CredentialMismatch(CredentialDimension::PrincipalRole),
    );
}

#[test]
fn registry_limit_accepts_maximum_and_rejects_one_over_before_lookup() {
    assert!(
        CredentialRegistrySnapshot::new(
            RevisionNumber::first(),
            registry_entries(MAX_CREDENTIAL_REGISTRY_ENTRIES),
        )
        .is_ok()
    );
    assert_eq!(
        CredentialRegistrySnapshot::new(
            RevisionNumber::first(),
            registry_entries(MAX_CREDENTIAL_REGISTRY_ENTRIES + 1),
        )
        .expect_err("one-over credential registry"),
        ApprovalError::CollectionTooLarge(CanonicalCollection::CredentialRegistry),
    );
}

#[test]
fn closed_risk_and_independence_cardinalities_block_one_over_values() {
    let risks = vec![
        RiskClass::Read,
        RiskClass::ScopedWrite,
        RiskClass::Execution,
        RiskClass::Network,
        RiskClass::DependencyEnvironment,
        RiskClass::RepositoryHistoryMutation,
        RiskClass::SecretUse,
        RiskClass::ExternalSideEffect,
        RiskClass::PolicyAuthority,
        RiskClass::HarnessPromotion,
    ];
    assert_eq!(risks.len(), MAX_RISK_CLASSES);
    assert!(RiskSet::new(risks.clone()).is_ok());
    let mut risk_one_over = risks;
    risk_one_over.push(RiskClass::HarnessPromotion);
    assert!(RiskSet::new(risk_one_over).is_err());

    let independence = vec![
        IndependenceRequirement::NotRequester,
        IndependenceRequirement::NotActionActor,
        IndependenceRequirement::NoProducingAttemptParticipation,
        IndependenceRequirement::NoReviewParticipation,
    ];
    assert_eq!(independence.len(), MAX_INDEPENDENCE_REQUIREMENTS);
    assert!(IndependenceSet::new(independence.clone()).is_ok());
    let mut independence_one_over = independence;
    independence_one_over.push(IndependenceRequirement::NoReviewParticipation);
    assert!(IndependenceSet::new(independence_one_over).is_err());
}

#[test]
fn amendment_identity_rejects_reused_successor_identity() {
    let policy = PolicyId::new([0x75; 16]).expect("policy");
    assert_eq!(
        AmendmentIdentity::new(policy, policy, PolicyTier::Project, Sha256Digest::new([0x76; 32]),)
            .expect_err("successor must be fresh"),
        ApprovalError::BindingMismatch(ScopeDimension::Policy),
    );
}
