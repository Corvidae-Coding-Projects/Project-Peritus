use ed25519_dalek::{Signer, SigningKey};
use peritus_approval::{
    ApprovalChoice, ApprovalDecision, ApprovalKeyId, ApprovalPublicKey, ApprovalSignature,
    ApproverCredential, CredentialRegistrySnapshot, CredentialStatus, SignedApprovalDecision,
};
use peritus_policy::{ActorRole, AuthorityInstant, AuthorityTier, ValidityWindow};
use peritus_types::{ActorId, CommandId, Generation, RevisionNumber};

use super::{SIGNING_SEED, SignedFixture, ids, instant, request, window};

pub fn signing_message(digest: peritus_approval::ApprovalDecisionDigest) -> Vec<u8> {
    let magic = b"PERITUS\0B1\0APPROVAL\0V1\0";
    let domain = b"approval-signed-decision";
    let mut bytes = Vec::new();
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(
        &u16::try_from(domain.len()).expect("fixed test domain fits u16").to_be_bytes(),
    );
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&32_u64.to_be_bytes());
    bytes.extend_from_slice(digest.sha256().as_bytes());
    bytes
}

pub fn approval_public_key() -> ApprovalPublicKey {
    ApprovalPublicKey::new(SigningKey::from_bytes(&SIGNING_SEED).verifying_key().to_bytes())
}

pub fn approval_key_id() -> ApprovalKeyId {
    ApprovalKeyId::compute(approval_public_key()).expect("bounded key ID")
}

#[allow(clippy::too_many_arguments)]
pub fn credential_registry(
    actor: ActorId,
    principal_role: ActorRole,
    allowed_roles: Vec<ActorRole>,
    maximum_tier: AuthorityTier,
    status: CredentialStatus,
    validity: ValidityWindow,
    generation: Generation,
    revision: RevisionNumber,
) -> CredentialRegistrySnapshot {
    let credential = ApproverCredential::new(
        approval_key_id(),
        approval_public_key(),
        actor,
        principal_role,
        ids().environment,
        ids().workspace,
        maximum_tier,
        allowed_roles,
        validity,
        generation,
        status,
    )
    .expect("credential shape");
    CredentialRegistrySnapshot::new(revision, vec![credential]).expect("registry")
}

#[allow(clippy::too_many_arguments)]
pub fn signed_decision(
    request: &peritus_approval::ApprovalRequest,
    choice: ApprovalChoice,
    responder: ActorId,
    role: ActorRole,
    key_id: ApprovalKeyId,
    generation: Generation,
    revision: RevisionNumber,
    command_byte: u8,
    expires_at: AuthorityInstant,
) -> SignedApprovalDecision {
    let decision = ApprovalDecision::new(
        CommandId::new([command_byte; 16]).expect("command"),
        responder,
        role,
        request.request_id(),
        request.digest(),
        choice,
        expires_at,
        key_id,
        generation,
        revision,
    )
    .expect("decision");
    let signature = SigningKey::from_bytes(&SIGNING_SEED).sign(&signing_message(decision.digest()));
    SignedApprovalDecision::new(decision, ApprovalSignature::new(signature.to_bytes()))
}

pub fn signed_fixture(choice: ApprovalChoice) -> SignedFixture {
    let fixture_ids = ids();
    let request = request(1, vec![peritus_policy::IndependenceRequirement::NotRequester]);
    let key_id = approval_key_id();
    let registry = credential_registry(
        fixture_ids.responder,
        ActorRole::HumanAuthority,
        vec![ActorRole::HumanAuthority],
        AuthorityTier::Organization,
        CredentialStatus::Enabled,
        window(10, 80),
        Generation::first(),
        RevisionNumber::first(),
    );
    let signed = signed_decision(
        &request,
        choice,
        fixture_ids.responder,
        ActorRole::HumanAuthority,
        key_id,
        Generation::first(),
        RevisionNumber::first(),
        16,
        instant(75),
    );
    SignedFixture { request, signed, registry, ids: fixture_ids, observed_at: instant(30) }
}
