//! Authenticated approve-once prerequisite for atomic promotion qualification.

use ed25519_dalek::{Signer, SigningKey};
use peritus_approval::{
    ActionDigest, ApprovalAggregate, ApprovalChoice, ApprovalDecision, ApprovalKeyId,
    ApprovalPublicKey, ApprovalRequest, ApprovalSignature, ApprovalUseOutcome, ApproverCredential,
    CredentialRegistrySnapshot, CredentialStatus, ParticipantSet, SignedApprovalDecision,
    verify_signed_decision,
};
use peritus_codec::{CodecLimits, encode_frame};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, ApprovalCommitRequest,
    CredentialRegistryInstall, CurrentCredentialRegistry, EventDraft, ExactFrame, HeadExpectation,
    SqliteJournal, StoreId,
};
use peritus_policy::{
    ActorRole, ActorSelector, ApprovalRequirement, AuthorityBoundary, AuthorityCeiling,
    AuthorityInstant, AuthorityTier, AuthorityTimeState, AuthorizationRequest, CapabilityScope,
    CeilingGrant, EnvironmentSelector, IndependenceRequirement, IndependenceSet, OperationClass,
    OperationDescriptor, OperationRegistry, Permission, PermissionSelector, PermissionSet,
    PolicyDefinition, PolicyTier, RestrictionLayer, RestrictionRule, RiskClass, RiskSet,
    RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_types::{
    ActionId, ActorId, ApprovalRequestId, CapabilityName, EnvironmentId, EventSequence, Generation,
    ResourceId, RevisionNumber, RevisionTuple,
};

use crate::{EvolutionError, PromotionProposal};

use super::identity::{command, digest, event, invalid, journal, nominal};

pub(super) struct PreparedApproval {
    pub(super) outcome: ApprovalUseOutcome,
    pub(super) current: CurrentCredentialRegistry,
    pub(super) request_id: ApprovalRequestId,
}

pub(super) fn prepare(
    journal_owner: &mut SqliteJournal,
    proposal: &PromotionProposal,
    revision: RevisionTuple,
    store: StoreId,
) -> Result<PreparedApproval, EvolutionError> {
    let fixture = ApprovalFixture::build(proposal, revision, store)?;
    let install = CredentialRegistryInstall::new(None, 1, &fixture.registry)
        .map_err(|_| invalid("construct qualification credential registry install"))?;
    journal_owner.commit_credential_registry(install).map_err(|_| journal())?;
    let current = journal_owner.current_credential_registry().map_err(|_| journal())?;
    let aggregate = ApprovalAggregate::new(fixture.request);
    let observation = verify_signed_decision(
        aggregate.request(),
        &fixture.decision,
        &fixture.registry,
        instant(30),
    )
    .map_err(|_| invalid("authenticate qualification approve-once decision"))?;
    let outcome = aggregate
        .resolve(observation, &fixture.registry)
        .map_err(|_| invalid("resolve qualification approve-once decision"))?;
    let append =
        approval_append(journal_owner.store_id(), outcome.aggregate().request().request_id())?;
    let committed = journal_owner
        .commit_approval_transition(
            ApprovalCommitRequest::new(append, outcome, None, Some(&current))
                .map_err(|_| journal())?,
        )
        .map_err(|_| journal())?;
    let approved = committed.into_parts().1;
    let request_id = approved.request().request_id();
    let action_id = approved.request().action_id();
    let action_digest = approved.request().action_digest();
    let outcome = approved
        .consume_once(action_id, action_digest, instant(40))
        .map_err(|_| invalid("consume qualification approve-once decision"))?;
    Ok(PreparedApproval { outcome, current, request_id })
}

struct ApprovalFixture {
    request: ApprovalRequest,
    decision: SignedApprovalDecision,
    registry: CredentialRegistrySnapshot,
}

impl ApprovalFixture {
    fn build(
        proposal: &PromotionProposal,
        revision: RevisionTuple,
        store: StoreId,
    ) -> Result<Self, EvolutionError> {
        let requester = ActorId::new(nominal(b"peritus/h1/promotion/requester/v1\0", store))
            .map_err(|_| invalid("construct qualification requester"))?;
        let responder = ActorId::new(nominal(b"peritus/h1/promotion/responder/v1\0", store))
            .map_err(|_| invalid("construct qualification responder"))?;
        let environment =
            EnvironmentId::new(nominal(b"peritus/h1/promotion/environment/v1\0", store))
                .map_err(|_| invalid("construct qualification environment"))?;
        let request_id =
            ApprovalRequestId::new(nominal(b"peritus/h1/promotion/approval-request/v1\0", store))
                .map_err(|_| invalid("construct qualification approval request"))?;
        let action_id = ActionId::new(nominal(b"peritus/h1/promotion/action/v1\0", store))
            .map_err(|_| invalid("construct qualification action"))?;
        let challenge = challenge(requester, environment, revision, store)?;
        let request = ApprovalRequest::new(
            request_id,
            action_id,
            ActionDigest::from_sha256(proposal.digest()),
            requester,
            ActorRole::Orchestrator,
            challenge,
            digest(b"peritus/h1/promotion/risk-details/v1\0", store),
            ParticipantSet::producing(Vec::new())
                .map_err(|_| invalid("construct qualification producing participants"))?,
            ParticipantSet::review(Vec::new())
                .map_err(|_| invalid("construct qualification review participants"))?,
            window(10, 90)?,
        )
        .map_err(|_| invalid("construct qualification approval request"))?;
        let signing = SigningKey::from_bytes(
            digest(b"peritus/h1/promotion/signing-key/v1\0", store).as_bytes(),
        );
        let public_key = ApprovalPublicKey::new(signing.verifying_key().to_bytes());
        let key_id = ApprovalKeyId::compute(public_key)
            .map_err(|_| invalid("compute qualification approval key identity"))?;
        let credential = ApproverCredential::new(
            key_id,
            public_key,
            responder,
            ActorRole::HumanAuthority,
            environment,
            revision.workspace_id(),
            AuthorityTier::Organization,
            vec![ActorRole::HumanAuthority],
            window(0, 100)?,
            Generation::first(),
            CredentialStatus::Enabled,
        )
        .map_err(|_| invalid("construct qualification approval credential"))?;
        let registry = CredentialRegistrySnapshot::new(RevisionNumber::first(), vec![credential])
            .map_err(|_| invalid("construct qualification credential registry"))?;
        let decision = ApprovalDecision::new(
            command(b"peritus/h1/promotion/approval-decision/v1\0", store)?,
            responder,
            ActorRole::HumanAuthority,
            request_id,
            request.digest(),
            ApprovalChoice::ApproveOnce,
            instant(75),
            key_id,
            Generation::first(),
            RevisionNumber::first(),
        )
        .map_err(|_| invalid("construct qualification approval decision"))?;
        let signature = signing.sign(&signing_message(decision.digest()));
        let decision =
            SignedApprovalDecision::new(decision, ApprovalSignature::new(signature.to_bytes()));
        Ok(Self { request, decision, registry })
    }
}

fn challenge(
    requester: ActorId,
    environment: EnvironmentId,
    revision: RevisionTuple,
    store: StoreId,
) -> Result<peritus_policy::EscalationChallenge, EvolutionError> {
    let boundary = AuthorityBoundary::new(
        vec![requester],
        vec![ActorRole::Orchestrator],
        vec![environment],
        permissions(store)?,
        revision,
        window(0, 100)?,
        UseLimit::limited(2).map_err(|_| invalid("construct qualification ceiling use limit"))?,
    )
    .map_err(|_| invalid("construct qualification authority boundary"))?;
    let ceiling_selector = selector(revision);
    let ceiling = AuthorityCeiling::new(
        boundary,
        vec![CeilingGrant::new(
            digest(b"peritus/h1/promotion/ceiling-grant/v1\0", store),
            ceiling_selector,
            window(5, 95)?,
            UseLimit::limited(1).map_err(|_| invalid("construct qualification grant use limit"))?,
        )],
        Vec::new(),
    )
    .map_err(|_| invalid("construct qualification authority ceiling"))?;
    let descriptor = OperationDescriptor::new(
        capability_name()?,
        OperationClass::HarnessPromotion,
        RiskSet::new(vec![RiskClass::HarnessPromotion])
            .map_err(|_| invalid("construct qualification promotion risk"))?,
    )
    .map_err(|_| invalid("construct qualification promotion operation"))?;
    let registry = OperationRegistry::new(vec![descriptor])
        .map_err(|_| invalid("construct qualification operation registry"))?;
    let requirement = ApprovalRequirement::new(
        AuthorityTier::User,
        vec![ActorRole::HumanAuthority],
        IndependenceSet::new(vec![IndependenceRequirement::NotRequester])
            .map_err(|_| invalid("construct qualification independence requirement"))?,
        window(10, 90)?,
    )
    .map_err(|_| invalid("construct qualification approval requirement"))?;
    let layer = RestrictionLayer::new(
        PolicyTier::Project,
        vec![RestrictionRule::require_approval(
            digest(b"peritus/h1/promotion/restriction/v1\0", store),
            selector(revision),
            requirement,
        )],
    )
    .map_err(|_| invalid("construct qualification restriction layer"))?;
    let policy = PolicyDefinition::new(revision.policy_id(), ceiling, registry, vec![layer])
        .map_err(|_| invalid("construct qualification promotion policy"))?;
    let scope = CapabilityScope::new(
        requester,
        ActorRole::Orchestrator,
        environment,
        permissions(store)?,
        revision,
        window(5, 95)?,
        UseLimit::limited(1).map_err(|_| invalid("construct qualification scope use limit"))?,
    );
    let decision = policy
        .evaluate(
            AuthorizationRequest::new(scope),
            AuthorityTimeState::new(instant(0)),
            instant(20),
        )
        .map_err(|_| invalid("evaluate qualification promotion policy"))?;
    let (plan, challenge, denial) = decision.into_parts();
    if plan.is_some() || denial.is_some() {
        return Err(invalid("qualification promotion policy did not require approval"));
    }
    challenge.ok_or_else(|| invalid("qualification promotion policy omitted its challenge"))
}

fn permissions(store: StoreId) -> Result<PermissionSet, EvolutionError> {
    let resource = ResourceId::new(nominal(b"peritus/h1/promotion/resource/v1\0", store))
        .map_err(|_| invalid("construct qualification promotion resource"))?;
    PermissionSet::new(vec![Permission::new(resource, capability_name()?)])
        .map_err(|_| invalid("construct qualification promotion permission set"))
}

fn capability_name() -> Result<CapabilityName, EvolutionError> {
    CapabilityName::new("harness.promote".to_owned())
        .map_err(|_| invalid("construct qualification promotion capability"))
}

const fn selector(revision: RevisionTuple) -> ScopeSelector {
    ScopeSelector::new(
        ActorSelector::any_within_parent(),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        revision,
    )
}

const fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}

fn window(start: u64, end: u64) -> Result<ValidityWindow, EvolutionError> {
    ValidityWindow::new(instant(start), instant(end))
        .map_err(|_| invalid("construct qualification authority window"))
}

fn signing_message(digest: peritus_approval::ApprovalDecisionDigest) -> Vec<u8> {
    let magic = b"PERITUS\0B1\0APPROVAL\0V1\0";
    let domain = b"approval-signed-decision";
    let mut bytes = Vec::new();
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(&u16::try_from(domain.len()).unwrap_or(u16::MAX).to_be_bytes());
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&32_u64.to_be_bytes());
    bytes.extend_from_slice(digest.sha256().as_bytes());
    bytes
}

fn approval_append(
    store: StoreId,
    request: ApprovalRequestId,
) -> Result<AppendRequest, EvolutionError> {
    let aggregate = AggregateKey::new(
        AggregateKind::Approval,
        AggregateId::new(nominal(b"peritus/h1/promotion/approval-aggregate/v1\0", store))
            .map_err(|_| invalid("construct qualification approval aggregate"))?,
    );
    let payload = request.as_bytes();
    let frame = ExactFrame::new(
        encode_frame(65_020, 1, payload, CodecLimits::PRODUCTION)
            .map_err(|_| invalid("encode qualification approval event"))?,
    )
    .map_err(|_| journal())?;
    let event = EventDraft::new(
        aggregate,
        EventSequence::first(),
        event(b"peritus/h1/promotion/approval-event/v1\0", store)?,
        None,
        frame,
        digest(b"peritus/h1/promotion/approval-state/v1\0", store),
        Vec::new(),
    )
    .map_err(|_| journal())?;
    Ok(AppendRequest::new(
        store,
        command(b"peritus/h1/promotion/approval-commit/v1\0", store)?,
        digest(b"peritus/h1/promotion/approval-request-digest/v1\0", store),
        vec![HeadExpectation::Absent(aggregate)],
        vec![event],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    ))
}
