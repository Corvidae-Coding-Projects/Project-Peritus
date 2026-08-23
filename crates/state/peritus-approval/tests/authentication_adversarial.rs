//! Credential, canonical-encoding, binding, and signature rejection matrix.

mod support;

use peritus_approval::{
    ApprovalChoice, ApprovalError, ApprovalKeyId, ApprovalPublicKey, ApprovalSignature,
    CredentialDimension, CredentialStatus, ScopeDimension, SignedApprovalDecision,
    verify_signed_decision,
};
use peritus_policy::{ActorRole, AuthorityTier, IndependenceRequirement};
use peritus_types::{ActorId, Generation, RevisionNumber};

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

#[test]
fn authentic_fixture_binds_human_principal_and_signed_role() {
    let fixture = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let observation = verify_signed_decision(
        &fixture.request,
        &fixture.signed,
        &fixture.registry,
        fixture.observed_at,
    )
    .expect("strict authentication");
    assert_eq!(observation.responder(), fixture.ids.responder);
    assert_eq!(observation.approver_role(), ActorRole::HumanAuthority);
    assert_eq!(observation.request_id(), fixture.request.request_id());
    assert_eq!(observation.request_digest(), fixture.request.digest());
    assert_eq!(observation.registry_revision(), RevisionNumber::first());
}

#[test]
fn registry_revision_generation_status_actor_and_tier_fail_exactly() {
    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let registry = support::credential_registry(
        base.ids.responder,
        ActorRole::HumanAuthority,
        vec![ActorRole::HumanAuthority],
        AuthorityTier::Organization,
        CredentialStatus::Enabled,
        support::window(10, 80),
        Generation::first(),
        RevisionNumber::new(2).expect("revision two"),
    );
    let fixture = support::SignedFixture { registry, ..base };
    assert_auth_error(
        &fixture,
        ApprovalError::CredentialMismatch(CredentialDimension::RegistryRevision),
    );

    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let signed = support::signed_decision(
        &base.request,
        ApprovalChoice::ApproveOnce,
        base.ids.responder,
        ActorRole::HumanAuthority,
        support::approval_key_id(),
        Generation::new(2).expect("generation two"),
        RevisionNumber::first(),
        21,
        support::instant(75),
    );
    let fixture = support::SignedFixture { signed, ..base };
    assert_auth_error(&fixture, ApprovalError::CredentialMismatch(CredentialDimension::Generation));

    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let registry = support::credential_registry(
        base.ids.responder,
        ActorRole::HumanAuthority,
        vec![ActorRole::HumanAuthority],
        AuthorityTier::Organization,
        CredentialStatus::Disabled,
        support::window(10, 80),
        Generation::first(),
        RevisionNumber::first(),
    );
    let fixture = support::SignedFixture { registry, ..base };
    assert_auth_error(&fixture, ApprovalError::CredentialMismatch(CredentialDimension::Status));

    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let registry = support::credential_registry(
        ActorId::new([0x33; 16]).expect("different actor"),
        ActorRole::HumanAuthority,
        vec![ActorRole::HumanAuthority],
        AuthorityTier::Organization,
        CredentialStatus::Enabled,
        support::window(10, 80),
        Generation::first(),
        RevisionNumber::first(),
    );
    let fixture = support::SignedFixture { registry, ..base };
    assert_auth_error(&fixture, ApprovalError::CredentialMismatch(CredentialDimension::Actor));

    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let registry = support::credential_registry(
        base.ids.responder,
        ActorRole::HumanAuthority,
        vec![ActorRole::HumanAuthority],
        AuthorityTier::Project,
        CredentialStatus::Enabled,
        support::window(10, 80),
        Generation::first(),
        RevisionNumber::first(),
    );
    let fixture = support::SignedFixture { registry, ..base };
    assert_auth_error(
        &fixture,
        ApprovalError::CredentialMismatch(CredentialDimension::AuthorityTier),
    );
}

#[test]
fn asserted_role_and_independence_are_conjoined_not_inferred() {
    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let signed = support::signed_decision(
        &base.request,
        ApprovalChoice::ApproveOnce,
        base.ids.responder,
        ActorRole::Writer,
        support::approval_key_id(),
        Generation::first(),
        RevisionNumber::first(),
        22,
        support::instant(75),
    );
    let registry = support::credential_registry(
        base.ids.responder,
        ActorRole::HumanAuthority,
        vec![ActorRole::Writer],
        AuthorityTier::Organization,
        CredentialStatus::Enabled,
        support::window(10, 80),
        Generation::first(),
        RevisionNumber::first(),
    );
    let fixture = support::SignedFixture { signed, registry, ..base };
    assert_auth_error(
        &fixture,
        ApprovalError::CredentialMismatch(CredentialDimension::ApprovalRole),
    );

    let request = support::request(1, vec![IndependenceRequirement::NotRequester]);
    let registry = support::credential_registry(
        request.requester(),
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
        request.requester(),
        ActorRole::HumanAuthority,
        support::approval_key_id(),
        Generation::first(),
        RevisionNumber::first(),
        23,
        support::instant(75),
    );
    let fixture = support::SignedFixture {
        request,
        signed,
        registry,
        ids: support::signed_fixture(ApprovalChoice::Deny).ids,
        observed_at: support::instant(30),
    };
    assert_auth_error(&fixture, ApprovalError::IndependenceViolation);
}

#[test]
fn request_binding_and_time_boundaries_fail_closed() {
    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let other = support::request(2, vec![IndependenceRequirement::NotRequester]);
    let signed = support::signed_decision(
        &other,
        ApprovalChoice::ApproveOnce,
        base.ids.responder,
        ActorRole::HumanAuthority,
        support::approval_key_id(),
        Generation::first(),
        RevisionNumber::first(),
        24,
        support::instant(75),
    );
    let fixture = support::SignedFixture { signed, ..base };
    assert_auth_error(&fixture, ApprovalError::BindingMismatch(ScopeDimension::RequestDigest));

    let mut fixture = support::signed_fixture(ApprovalChoice::ApproveOnce);
    fixture.observed_at = support::instant(75);
    assert_auth_error(&fixture, ApprovalError::Expired);

    let mut fixture = support::signed_fixture(ApprovalChoice::ApproveOnce);
    fixture.observed_at = support::instant(9);
    assert_auth_error(&fixture, ApprovalError::ClockRegression);
}

#[test]
fn weak_public_keys_noncanonical_scalars_and_mutations_are_rejected() {
    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let weak_key = ApprovalPublicKey::new([0; 32]);
    let weak_key_id = ApprovalKeyId::compute(weak_key).expect("bounded weak-key identifier");
    let registry = peritus_approval::CredentialRegistrySnapshot::new(
        RevisionNumber::first(),
        vec![
            peritus_approval::ApproverCredential::new(
                weak_key_id,
                weak_key,
                base.ids.responder,
                ActorRole::HumanAuthority,
                base.ids.environment,
                base.ids.workspace,
                AuthorityTier::Organization,
                vec![ActorRole::HumanAuthority],
                support::window(10, 80),
                Generation::first(),
                CredentialStatus::Enabled,
            )
            .expect("structurally bound weak credential"),
        ],
    )
    .expect("registry");
    let signed = support::signed_decision(
        &base.request,
        ApprovalChoice::ApproveOnce,
        base.ids.responder,
        ActorRole::HumanAuthority,
        weak_key_id,
        Generation::first(),
        RevisionNumber::first(),
        25,
        support::instant(75),
    );
    let fixture = support::SignedFixture { signed, registry, ..base };
    assert_auth_error(&fixture, ApprovalError::InvalidCryptoEncoding);

    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let (decision, _) = base.signed.into_parts();
    let mut bytes = [0_u8; 64];
    bytes[..32].copy_from_slice(&[1; 32]);
    bytes[32..].copy_from_slice(&[
        0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde,
        0x14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
    ]);
    let signed = SignedApprovalDecision::new(decision, ApprovalSignature::new(bytes));
    let fixture = support::SignedFixture { signed, ..base };
    assert_auth_error(&fixture, ApprovalError::InvalidCryptoEncoding);

    let base = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let (decision, signature) = base.signed.into_parts();
    let mut bytes = *signature.as_bytes();
    bytes[40] ^= 1;
    let signed = SignedApprovalDecision::new(decision, ApprovalSignature::new(bytes));
    let fixture = support::SignedFixture { signed, ..base };
    assert_auth_error(&fixture, ApprovalError::SignatureInvalid);
}
