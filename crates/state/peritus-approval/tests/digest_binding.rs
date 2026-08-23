//! Field-sensitive request, decision, and amendment digest binding evidence.

mod support;

use peritus_approval::{
    AmendmentIdentity, ApprovalChoice, ApprovalDecision, ApprovalError, ApprovalKeyId,
    ApprovalPublicKey, ApprovalSignature, CredentialDimension, ScopeDimension,
    SignedApprovalDecision, verify_signed_decision,
};
use peritus_policy::{ActorRole, IndependenceRequirement, PolicyTier};
use peritus_types::{
    ActorId, ApprovalRequestId, CommandId, Generation, PolicyId, RevisionNumber, Sha256Digest,
};

#[allow(
    clippy::too_many_arguments,
    reason = "the test varies every signed decision field independently"
)]
fn decision(
    _fixture: &support::SignedFixture,
    command_id: CommandId,
    responder: ActorId,
    role: ActorRole,
    request_id: ApprovalRequestId,
    request_digest: peritus_approval::ApprovalRequestDigest,
    choice: ApprovalChoice,
    expires_tick: u64,
    key_id: ApprovalKeyId,
    generation: Generation,
    revision: RevisionNumber,
) -> ApprovalDecision {
    ApprovalDecision::new(
        command_id,
        responder,
        role,
        request_id,
        request_digest,
        choice,
        support::instant(expires_tick),
        key_id,
        generation,
        revision,
    )
    .expect("bounded decision")
}

fn assert_old_signature_rejected(
    fixture: &support::SignedFixture,
    decision: ApprovalDecision,
    signature: ApprovalSignature,
    expected: ApprovalError,
) {
    assert_ne!(decision.digest(), fixture.signed.decision().digest());
    let signed = SignedApprovalDecision::new(decision, signature);
    let actual =
        verify_signed_decision(&fixture.request, &signed, &fixture.registry, fixture.observed_at)
            .expect_err("changed signed field must fail");
    assert_eq!(actual, expected);
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the table-free mutation matrix keeps each signed field and exact rejection adjacent"
)]
fn every_signed_decision_field_changes_the_digest_and_rejects_the_old_signature() {
    let fixture = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let original = fixture.signed.decision();
    let signature = fixture.signed.signature();
    let command_id = original.command_id();
    let responder = original.responder();
    let role = original.approver_role();
    let request_id = original.request_id();
    let request_digest = original.request_digest();
    let choice = original.choice();
    let key_id = original.key_id();
    let generation = original.credential_generation();
    let revision = original.registry_revision();

    assert_old_signature_rejected(
        &fixture,
        decision(
            &fixture,
            CommandId::new([0x51; 16]).expect("other command"),
            responder,
            role,
            request_id,
            request_digest,
            choice,
            75,
            key_id,
            generation,
            revision,
        ),
        signature,
        ApprovalError::SignatureInvalid,
    );
    assert_old_signature_rejected(
        &fixture,
        decision(
            &fixture,
            command_id,
            ActorId::new([0x52; 16]).expect("other responder"),
            role,
            request_id,
            request_digest,
            choice,
            75,
            key_id,
            generation,
            revision,
        ),
        signature,
        ApprovalError::CredentialMismatch(CredentialDimension::Actor),
    );
    assert_old_signature_rejected(
        &fixture,
        decision(
            &fixture,
            command_id,
            responder,
            ActorRole::Writer,
            request_id,
            request_digest,
            choice,
            75,
            key_id,
            generation,
            revision,
        ),
        signature,
        ApprovalError::CredentialMismatch(CredentialDimension::ApprovalRole),
    );
    assert_old_signature_rejected(
        &fixture,
        decision(
            &fixture,
            command_id,
            responder,
            role,
            ApprovalRequestId::new([0x53; 16]).expect("other request"),
            request_digest,
            choice,
            75,
            key_id,
            generation,
            revision,
        ),
        signature,
        ApprovalError::BindingMismatch(ScopeDimension::Request),
    );
    let other_request = support::request(2, Vec::new());
    assert_old_signature_rejected(
        &fixture,
        decision(
            &fixture,
            command_id,
            responder,
            role,
            request_id,
            other_request.digest(),
            choice,
            75,
            key_id,
            generation,
            revision,
        ),
        signature,
        ApprovalError::BindingMismatch(ScopeDimension::RequestDigest),
    );
    assert_old_signature_rejected(
        &fixture,
        decision(
            &fixture,
            command_id,
            responder,
            role,
            request_id,
            request_digest,
            ApprovalChoice::Deny,
            75,
            key_id,
            generation,
            revision,
        ),
        signature,
        ApprovalError::SignatureInvalid,
    );
    assert_old_signature_rejected(
        &fixture,
        decision(
            &fixture,
            command_id,
            responder,
            role,
            request_id,
            request_digest,
            choice,
            74,
            key_id,
            generation,
            revision,
        ),
        signature,
        ApprovalError::SignatureInvalid,
    );
    let other_key = ApprovalKeyId::compute(ApprovalPublicKey::new([0x54; 32])).expect("other key");
    assert_old_signature_rejected(
        &fixture,
        decision(
            &fixture,
            command_id,
            responder,
            role,
            request_id,
            request_digest,
            choice,
            75,
            other_key,
            generation,
            revision,
        ),
        signature,
        ApprovalError::CredentialMissing,
    );
    assert_old_signature_rejected(
        &fixture,
        decision(
            &fixture,
            command_id,
            responder,
            role,
            request_id,
            request_digest,
            choice,
            75,
            key_id,
            Generation::new(2).expect("other generation"),
            revision,
        ),
        signature,
        ApprovalError::CredentialMismatch(CredentialDimension::Generation),
    );
    assert_old_signature_rejected(
        &fixture,
        decision(
            &fixture,
            command_id,
            responder,
            role,
            request_id,
            request_digest,
            choice,
            75,
            key_id,
            generation,
            RevisionNumber::new(2).expect("other revision"),
        ),
        signature,
        ApprovalError::CredentialMismatch(CredentialDimension::RegistryRevision),
    );
}

#[test]
fn request_collections_and_redacted_risk_details_are_digest_bound() {
    let baseline = support::request(1, Vec::new());
    assert_ne!(baseline.digest(), support::request(2, Vec::new()).digest());
    assert_ne!(
        baseline.digest(),
        support::request(1, vec![IndependenceRequirement::NotRequester]).digest(),
    );
    assert_ne!(
        baseline.digest(),
        support::request_with_risk_digest(1, Vec::new(), Sha256Digest::new([0x55; 32])).digest(),
    );
    let ids = support::ids();
    assert_ne!(
        baseline.digest(),
        support::request_with_participants(Vec::new(), vec![ids.responder], Vec::new()).digest(),
    );
    assert_ne!(
        baseline.digest(),
        support::request_with_participants(Vec::new(), Vec::new(), vec![ids.responder]).digest(),
    );
}

#[test]
fn all_four_amendment_identity_fields_change_the_decision_digest() {
    let fixture = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let base = PolicyId::new([0x61; 16]).expect("base policy");
    let successor = PolicyId::new([0x62; 16]).expect("successor policy");
    let identity =
        AmendmentIdentity::new(base, successor, PolicyTier::Project, Sha256Digest::new([0x63; 32]))
            .expect("identity");
    let digest_for = |identity| {
        decision(
            &fixture,
            fixture.signed.decision().command_id(),
            fixture.ids.responder,
            ActorRole::HumanAuthority,
            fixture.request.request_id(),
            fixture.request.digest(),
            ApprovalChoice::Amend(identity),
            75,
            support::approval_key_id(),
            Generation::first(),
            RevisionNumber::first(),
        )
        .digest()
    };
    let baseline = digest_for(identity);
    let variants = [
        AmendmentIdentity::new(
            PolicyId::new([0x64; 16]).expect("other base"),
            successor,
            PolicyTier::Project,
            Sha256Digest::new([0x63; 32]),
        )
        .expect("variant"),
        AmendmentIdentity::new(
            base,
            PolicyId::new([0x65; 16]).expect("other successor"),
            PolicyTier::Project,
            Sha256Digest::new([0x63; 32]),
        )
        .expect("variant"),
        AmendmentIdentity::new(base, successor, PolicyTier::User, Sha256Digest::new([0x63; 32]))
            .expect("variant"),
        AmendmentIdentity::new(base, successor, PolicyTier::Project, Sha256Digest::new([0x66; 32]))
            .expect("variant"),
    ];
    for variant in variants {
        assert_ne!(baseline, digest_for(variant));
    }
}
