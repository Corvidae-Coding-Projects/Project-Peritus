#![allow(
    dead_code,
    reason = "shared integration support is compiled independently for each focused test binary"
)]
use peritus_approval::{
    AmendmentIdentity, ApprovalRequest, CredentialRegistrySnapshot, SignedApprovalDecision,
};
use peritus_policy::{
    ActorRole, AuthorityInstant, AuthorityTier, Permission, PermissionSet, PolicyRevisionCandidate,
    ValidityWindow,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, ApprovalRequestId, EnvironmentId, Generation, HarnessId,
    PolicyId, ProviderProfileId, ResourceId, RevisionNumber, RevisionTuple, WorkspaceId,
};

mod amendment;
mod request_fixture;
mod signing;

#[allow(
    unused_imports,
    reason = "shared integration support exposes one fixture surface to independently compiled suites"
)]
pub use request_fixture::{
    challenge_with_operation_risks, request, request_result, request_result_with_risk_digest,
    request_with_participants, request_with_permission_and_participants, request_with_risk_digest,
};

pub const SIGNING_SEED: [u8; 32] = [7; 32];

pub fn signing_message(digest: peritus_approval::ApprovalDecisionDigest) -> Vec<u8> {
    signing::signing_message(digest)
}

pub fn approval_public_key() -> peritus_approval::ApprovalPublicKey {
    signing::approval_public_key()
}

pub fn approval_key_id() -> peritus_approval::ApprovalKeyId {
    signing::approval_key_id()
}

#[allow(clippy::too_many_arguments)]
pub fn credential_registry(
    actor: ActorId,
    principal_role: ActorRole,
    allowed_roles: Vec<ActorRole>,
    maximum_tier: AuthorityTier,
    status: peritus_approval::CredentialStatus,
    validity: ValidityWindow,
    generation: Generation,
    revision: RevisionNumber,
) -> CredentialRegistrySnapshot {
    signing::credential_registry(
        actor,
        principal_role,
        allowed_roles,
        maximum_tier,
        status,
        validity,
        generation,
        revision,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn signed_decision(
    request: &ApprovalRequest,
    choice: peritus_approval::ApprovalChoice,
    responder: ActorId,
    role: ActorRole,
    key_id: peritus_approval::ApprovalKeyId,
    generation: Generation,
    revision: RevisionNumber,
    command_byte: u8,
    expires_at: AuthorityInstant,
) -> SignedApprovalDecision {
    signing::signed_decision(
        request,
        choice,
        responder,
        role,
        key_id,
        generation,
        revision,
        command_byte,
        expires_at,
    )
}

pub fn signed_fixture(choice: peritus_approval::ApprovalChoice) -> SignedFixture {
    signing::signed_fixture(choice)
}

#[derive(Clone, Copy)]
pub struct FixtureIds {
    pub requester: ActorId,
    pub responder: ActorId,
    pub environment: EnvironmentId,
    pub workspace: WorkspaceId,
    pub request: ApprovalRequestId,
    pub action: ActionId,
    pub revision: RevisionTuple,
}

pub struct SignedFixture {
    pub request: ApprovalRequest,
    pub signed: SignedApprovalDecision,
    pub registry: CredentialRegistrySnapshot,
    pub ids: FixtureIds,
    pub observed_at: AuthorityInstant,
}

pub const fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}

pub fn window(start: u64, end: u64) -> ValidityWindow {
    ValidityWindow::new(instant(start), instant(end)).expect("same-epoch valid window")
}

fn capability_name() -> peritus_types::CapabilityName {
    peritus_types::CapabilityName::new("workspace.inspect".to_owned())
        .expect("canonical capability")
}

fn resource(index: usize) -> ResourceId {
    let mut bytes = [0_u8; 16];
    bytes[0] = 0x40;
    bytes[8..].copy_from_slice(&(index as u64 + 1).to_be_bytes());
    ResourceId::new(bytes).expect("nonzero resource")
}

fn permissions(count: usize) -> PermissionSet {
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        values.push(Permission::new(resource(index), capability_name()));
    }
    PermissionSet::new(values).expect("strict resource order")
}

pub fn ids() -> FixtureIds {
    let requester = ActorId::new([1; 16]).expect("requester");
    let responder = ActorId::new([2; 16]).expect("responder");
    let environment = EnvironmentId::new([3; 16]).expect("environment");
    let workspace = WorkspaceId::new([4; 16]).expect("workspace");
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new([5; 16]).expect("acceptance"),
        HarnessId::new([6; 16]).expect("harness"),
        workspace,
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([8; 16]).expect("policy"),
        ProviderProfileId::new([9; 16]).expect("provider"),
    );
    FixtureIds {
        requester,
        responder,
        environment,
        workspace,
        request: ApprovalRequestId::new([10; 16]).expect("request"),
        action: ActionId::new([11; 16]).expect("action"),
        revision,
    }
}

pub fn amendment_candidate() -> (PolicyRevisionCandidate, AmendmentIdentity) {
    amendment::amendment_candidate()
}

pub fn amendment_candidate_with(
    successor_byte: u8,
    digest_byte: u8,
) -> (PolicyRevisionCandidate, AmendmentIdentity) {
    amendment::amendment_candidate_with(successor_byte, digest_byte)
}
