//! Remaining credential dimensions, clock discontinuity, and independence categories.

mod support;

use ed25519_dalek::{Signer, SigningKey};
use peritus_approval::{
    ApprovalChoice, ApprovalDecision, ApprovalError, ApprovalSignature, ApproverCredential,
    CredentialDimension, CredentialRegistrySnapshot, CredentialStatus, ScopeDimension,
    SignedApprovalDecision, verify_signed_decision,
};
use peritus_policy::{ActorRole, AuthorityInstant, AuthorityTier, IndependenceRequirement};
use peritus_types::{
    ActorId, ApprovalRequestId, CommandId, EnvironmentId, Generation, RevisionNumber, WorkspaceId,
};

fn assert_auth_error(fixture: &support::SignedFixture, expected: ApprovalError) {
    let actual = verify_signed_decision(
        &fixture.request,
        &fixture.signed,
        &fixture.registry,
        fixture.observed_at,
    )
    .expect_err("adversarial input must fail");
    assert_eq!(actual, expected);
}

fn scoped_registry(
    actor: ActorId,
    environment: EnvironmentId,
    workspace: WorkspaceId,
) -> CredentialRegistrySnapshot {
    let credential = ApproverCredential::new(
        support::approval_key_id(),
        support::approval_public_key(),
        actor,
        ActorRole::HumanAuthority,
        environment,
        workspace,
        AuthorityTier::Organization,
        vec![ActorRole::HumanAuthority],
        support::window(10, 80),
        Generation::first(),
        CredentialStatus::Enabled,
    )
    .expect("structural credential");
    CredentialRegistrySnapshot::new(RevisionNumber::first(), vec![credential]).expect("registry")
}

#[test]
fn missing_and_wrong_scope_credentials_fail_exactly() {
    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let registry = CredentialRegistrySnapshot::new(RevisionNumber::first(), Vec::new())
        .expect("empty supplied snapshot is valid");
    let fixture = support::SignedFixture { registry, ..base };
    assert_auth_error(&fixture, ApprovalError::CredentialMissing);

    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let registry = scoped_registry(
        base.ids.responder,
        EnvironmentId::new([0x41; 16]).expect("other environment"),
        base.ids.workspace,
    );
    let fixture = support::SignedFixture { registry, ..base };
    assert_auth_error(
        &fixture,
        ApprovalError::CredentialMismatch(CredentialDimension::Environment),
    );

    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let registry = scoped_registry(
        base.ids.responder,
        base.ids.environment,
        WorkspaceId::new([0x42; 16]).expect("other workspace"),
    );
    let fixture = support::SignedFixture { registry, ..base };
    assert_auth_error(&fixture, ApprovalError::CredentialMismatch(CredentialDimension::Workspace));
}

#[test]
fn credential_validity_and_clock_epoch_are_half_open_and_fail_closed() {
    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let registry = support::credential_registry(
        base.ids.responder,
        ActorRole::HumanAuthority,
        vec![ActorRole::HumanAuthority],
        AuthorityTier::Organization,
        CredentialStatus::Enabled,
        support::window(40, 80),
        Generation::first(),
        RevisionNumber::first(),
    );
    let fixture = support::SignedFixture { registry, ..base };
    assert_auth_error(&fixture, ApprovalError::NotYetValid);

    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let registry = support::credential_registry(
        base.ids.responder,
        ActorRole::HumanAuthority,
        vec![ActorRole::HumanAuthority],
        AuthorityTier::Organization,
        CredentialStatus::Enabled,
        support::window(10, 30),
        Generation::first(),
        RevisionNumber::first(),
    );
    let fixture = support::SignedFixture { registry, ..base };
    assert_auth_error(&fixture, ApprovalError::Expired);

    let mut fixture = support::signed_fixture(ApprovalChoice::ApproveOnce);
    fixture.observed_at = AuthorityInstant::new(
        Generation::new(2).expect("different clock epoch"),
        fixture.observed_at.tick_millis(),
    );
    assert_auth_error(&fixture, ApprovalError::ClockEpochMismatch);
}

#[test]
fn delayed_decision_from_the_same_reissued_key_fails_on_generation() {
    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let registry = support::credential_registry(
        base.ids.responder,
        ActorRole::HumanAuthority,
        vec![ActorRole::HumanAuthority],
        AuthorityTier::Organization,
        CredentialStatus::Enabled,
        support::window(10, 80),
        Generation::new(2).expect("reissued generation"),
        RevisionNumber::new(2).expect("new registry revision"),
    );
    let signed = support::signed_decision(
        &base.request,
        ApprovalChoice::ApproveOnce,
        base.ids.responder,
        ActorRole::HumanAuthority,
        support::approval_key_id(),
        Generation::first(),
        RevisionNumber::new(2).expect("current signed snapshot"),
        0x46,
        support::instant(75),
    );
    let fixture = support::SignedFixture { signed, registry, ..base };
    assert_auth_error(&fixture, ApprovalError::CredentialMismatch(CredentialDimension::Generation));
}

#[test]
fn signed_request_identity_mismatch_is_rejected_before_state_transition() {
    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let decision = ApprovalDecision::new(
        CommandId::new([0x43; 16]).expect("command"),
        base.ids.responder,
        ActorRole::HumanAuthority,
        ApprovalRequestId::new([0x44; 16]).expect("other request"),
        base.request.digest(),
        ApprovalChoice::ApproveOnce,
        support::instant(75),
        support::approval_key_id(),
        Generation::first(),
        RevisionNumber::first(),
    )
    .expect("digest-bound adversarial decision");
    let signature = SigningKey::from_bytes(&support::SIGNING_SEED)
        .sign(&support::signing_message(decision.digest()));
    let signed =
        SignedApprovalDecision::new(decision, ApprovalSignature::new(signature.to_bytes()));
    let fixture = support::SignedFixture { signed, ..base };
    assert_auth_error(&fixture, ApprovalError::BindingMismatch(ScopeDimension::Request));
}

fn assert_independence_conflict(
    requirement: IndependenceRequirement,
    responder: ActorId,
    producing: Vec<ActorId>,
    review: Vec<ActorId>,
) {
    let ids = support::ids();
    let request = support::request_with_participants(vec![requirement], producing, review);
    let registry = support::credential_registry(
        responder,
        ActorRole::HumanAuthority,
        vec![ActorRole::HumanAuthority],
        AuthorityTier::Organization,
        CredentialStatus::Enabled,
        support::window(10, 80),
        Generation::first(),
        RevisionNumber::first(),
    );
    let signed = support::signed_decision(
        &request,
        ApprovalChoice::ApproveOnce,
        responder,
        ActorRole::HumanAuthority,
        support::approval_key_id(),
        Generation::first(),
        RevisionNumber::first(),
        0x45,
        support::instant(75),
    );
    let fixture = support::SignedFixture {
        request,
        signed,
        registry,
        ids,
        observed_at: support::instant(30),
    };
    assert_auth_error(&fixture, ApprovalError::IndependenceViolation);
}

#[test]
fn every_independence_category_rejects_its_exact_conflict() {
    let ids = support::ids();
    assert_independence_conflict(
        IndependenceRequirement::NotRequester,
        ids.requester,
        Vec::new(),
        Vec::new(),
    );
    assert_independence_conflict(
        IndependenceRequirement::NotActionActor,
        ids.requester,
        Vec::new(),
        Vec::new(),
    );
    assert_independence_conflict(
        IndependenceRequirement::NoProducingAttemptParticipation,
        ids.responder,
        vec![ids.responder],
        Vec::new(),
    );
    assert_independence_conflict(
        IndependenceRequirement::NoReviewParticipation,
        ids.responder,
        Vec::new(),
        vec![ids.responder],
    );
}
