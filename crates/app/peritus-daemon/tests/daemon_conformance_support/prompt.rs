//! Durable approval-prompt freshness exercised through the public A3 process boundary.

use std::io;

use peritus_app_protocol::{
    AppErrorCode, AppMessage, AppProtocolLimits, AppRequestPayload, AppResponsePayload,
    ApprovalChallenge, PromptAnswer, PromptAnswerPayload, PromptBinding, PromptCancellation,
    PromptCorrelation, PromptId, SignedApprovalDecisionFrame, encode_prompt_binding_value,
};
use peritus_approval::{ActionDigest, ApprovalRequest, ParticipantSet, encode_approval_request};
use peritus_conformance::{
    DaemonConformanceObservation, DaemonPromptObservation, DaemonPromptRejection,
};
use peritus_journal::{
    ApplicationPromptId, ApplicationPromptRegistration, ApplicationPromptTargetKind,
    ApplicationRequestId, NewApplicationPromptTarget, SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_policy::{
    ActorRole, ActorSelector, ApprovalRequirement, AuthorityBoundary, AuthorityCeiling,
    AuthorityInstant, AuthorityTimeState, AuthorizationRequest, CapabilityScope, CeilingGrant,
    EnvironmentSelector, IndependenceSet, OperationClass, OperationDescriptor, OperationRegistry,
    Permission, PermissionSelector, PermissionSet, PolicyDefinition, PolicyTier, RestrictionLayer,
    RestrictionRule, RiskClass, RiskSet, RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, ApprovalRequestId, CapabilityName, CommandId,
    EnvironmentId, Generation, HarnessId, PolicyId, ProviderProfileId, ResourceId, RevisionNumber,
    RevisionTuple, SessionId, Sha256Digest, WorkspaceId,
};

use super::process::TestEnvironment;
use super::session::{fresh_hello, resume_hello};
use super::wire::WireClient;

pub(super) fn freshness() -> io::Result<DaemonConformanceObservation> {
    let environment = TestEnvironment::new()?;
    let mut initial = environment.start()?;
    let client = WireClient::establish(initial.endpoint(), fresh_hello(214))?;
    let session_id = client.context().session_id();
    drop(client);
    initial.kill_for_restart()?;

    let binding = seed_prompt(&environment, session_id)?;
    let mut restarted = environment.start()?;
    let mut client = WireClient::establish(restarted.endpoint(), resume_hello(215, session_id))?;
    let current = binding.correlation();
    let mut rejected = Vec::new();

    let wrong_actor = correlation(
        current,
        ActorId::new([0x23; 16]).map_err(super::debug_error)?,
        current.revision(),
        current.cancellation_generation(),
    );
    if rejected_answer(&mut client, wrong_actor, 216, AppErrorCode::PromptMismatch)? {
        rejected.push(DaemonPromptRejection::ActorSessionMismatch);
    }

    let stale_revision = RevisionTuple::new(
        current.revision().acceptance_spec_id(),
        current.revision().harness_id(),
        current.revision().workspace_id(),
        current.revision().workspace_generation(),
        RevisionNumber::new(2).map_err(super::debug_error)?,
        current.revision().policy_id(),
        current.revision().provider_profile_id(),
    );
    let stale =
        correlation(current, current.actor_id(), stale_revision, current.cancellation_generation());
    if rejected_answer(&mut client, stale, 217, AppErrorCode::PromptStale)? {
        rejected.push(DaemonPromptRejection::StaleRevisionGeneration);
    }

    if rejected_answer(&mut client, current, 218, AppErrorCode::PromptMismatch)? {
        rejected.push(DaemonPromptRejection::UnsignedApproval);
    }

    let cancellation_id =
        peritus_app_protocol::CorrelationId::new([219; 16]).map_err(super::debug_error)?;
    let cancellation = PromptCancellation::new(current, cancellation_id);
    let settled = matches!(
        client.request_bound(
            AppRequestPayload::CancelPrompt(cancellation),
            peritus_app_protocol::RequestId::new([220; 16]).map_err(super::debug_error)?,
            cancellation_id,
        )?,
        AppMessage::Response(response)
            if matches!(response.payload(), AppResponsePayload::PromptAccepted(id) if *id == current.prompt_id())
    );
    drop(client);
    restarted.kill_for_restart()?;

    let journal = open_journal(&environment)?;
    let record = journal
        .application_prompt_target(
            ApplicationPromptId::new(*current.prompt_id().as_bytes())
                .map_err(super::debug_error)?,
        )
        .map_err(super::debug_error)?
        .ok_or_else(|| io::Error::other("durable prompt target disappeared"))?;
    Ok(DaemonConformanceObservation::Prompt(DaemonPromptObservation::new(
        settled,
        rejected,
        u64::from(record.settlement().is_some()),
    )))
}

fn seed_prompt(environment: &TestEnvironment, session_id: SessionId) -> io::Result<PromptBinding> {
    let mut journal = open_journal(environment)?;
    let current_epoch = journal
        .current_authority_epoch()
        .map_err(super::debug_error)?
        .ok_or_else(|| io::Error::other("daemon did not allocate an authority epoch"))?
        .get();
    let next_epoch = current_epoch
        .checked_add(1)
        .ok_or_else(|| io::Error::other("authority epoch exhausted"))?;
    let actor_id = ActorId::new([0x22; 16]).map_err(super::debug_error)?;
    let revision = revision();
    let request = approval_request(actor_id, revision, next_epoch)?;
    let correlation = PromptCorrelation::new(
        peritus_app_protocol::RequestId::new([221; 16]).map_err(super::debug_error)?,
        PromptId::new([222; 16]).map_err(super::debug_error)?,
        session_id,
        actor_id,
        revision,
        request.digest().sha256(),
        Generation::first(),
    );
    let request_frame = encode_approval_request(&request).map_err(super::debug_error)?;
    let challenge = ApprovalChallenge::new(
        CommandId::new([223; 16]).map_err(super::debug_error)?,
        RevisionNumber::first(),
        request_frame,
        AppProtocolLimits::PRODUCTION.codec().max_opaque_bytes,
    )
    .map_err(super::debug_error)?;
    let binding = PromptBinding::approval(correlation, challenge, Vec::new(), 1)
        .map_err(super::debug_error)?;
    let binding_bytes = encode_prompt_binding_value(&binding, AppProtocolLimits::PRODUCTION)
        .map_err(super::debug_error)?;
    let target = NewApplicationPromptTarget::new(
        ApplicationPromptId::new(*correlation.prompt_id().as_bytes())
            .map_err(super::debug_error)?,
        actor_id,
        session_id,
        ApplicationRequestId::new(*correlation.originating_request_id().as_bytes())
            .map_err(super::debug_error)?,
        ApplicationPromptTargetKind::Approval,
        revision,
        correlation.freshness_digest(),
        correlation.cancellation_generation(),
        peritus_codec::sha256(&binding_bytes),
        binding_bytes,
        1024,
    )
    .map_err(super::debug_error)?;
    if !matches!(
        journal.register_application_prompt_target(target).map_err(super::debug_error)?,
        ApplicationPromptRegistration::Inserted(_)
    ) {
        return Err(io::Error::other("prompt target was not inserted exactly once"));
    }
    Ok(binding)
}

fn rejected_answer(
    client: &mut WireClient,
    correlation: PromptCorrelation,
    seed: u8,
    expected: AppErrorCode,
) -> io::Result<bool> {
    let decision = SignedApprovalDecisionFrame::new(
        vec![seed],
        AppProtocolLimits::PRODUCTION.codec().max_opaque_bytes,
    )
    .map_err(super::debug_error)?;
    let payload = PromptAnswerPayload::signed_approval(
        decision,
        None,
        AppProtocolLimits::PRODUCTION.codec().max_string_bytes,
    )
    .map_err(super::debug_error)?;
    let answer = PromptAnswer::new(
        correlation,
        payload,
        AppProtocolLimits::PRODUCTION.codec().max_string_bytes,
    )
    .map_err(super::debug_error)?;
    Ok(matches!(
        client.request(seed, AppRequestPayload::AnswerPrompt(answer))?,
        AppMessage::Response(response)
            if matches!(response.payload(), AppResponsePayload::Error(error) if error.code() == expected)
    ))
}

fn correlation(
    current: PromptCorrelation,
    actor_id: ActorId,
    revision: RevisionTuple,
    cancellation_generation: Generation,
) -> PromptCorrelation {
    PromptCorrelation::new(
        current.originating_request_id(),
        current.prompt_id(),
        current.session_id(),
        actor_id,
        revision,
        current.freshness_digest(),
        cancellation_generation,
    )
}

fn open_journal(environment: &TestEnvironment) -> io::Result<SqliteJournal> {
    SqliteJournal::open(
        environment.database_path(),
        StoreId::new([0x11; 16]).map_err(super::debug_error)?,
        SqliteJournalOptions::default(),
    )
    .map_err(super::debug_error)
}

fn approval_request(
    actor_id: ActorId,
    revision: RevisionTuple,
    epoch: u64,
) -> io::Result<ApprovalRequest> {
    let environment = EnvironmentId::new([224; 16]).map_err(super::debug_error)?;
    let boundary = AuthorityBoundary::new(
        vec![actor_id],
        vec![ActorRole::Writer],
        vec![environment],
        permissions()?,
        revision,
        window(epoch, 0, 60_000)?,
        UseLimit::limited(2).map_err(super::debug_error)?,
    )
    .map_err(super::debug_error)?;
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
            Sha256Digest::new([225; 32]),
            selector,
            window(epoch, 0, 60_000)?,
            UseLimit::limited(1).map_err(super::debug_error)?,
        )],
        Vec::new(),
    )
    .map_err(super::debug_error)?;
    let descriptor = OperationDescriptor::new(
        capability_name()?,
        OperationClass::Inspection,
        RiskSet::new(vec![RiskClass::Read]).map_err(super::debug_error)?,
    )
    .map_err(super::debug_error)?;
    let registry = OperationRegistry::new(vec![descriptor]).map_err(super::debug_error)?;
    let requirement = ApprovalRequirement::new(
        peritus_policy::AuthorityTier::User,
        vec![ActorRole::HumanAuthority],
        IndependenceSet::new(Vec::new()).map_err(super::debug_error)?,
        window(epoch, 0, 60_000)?,
    )
    .map_err(super::debug_error)?;
    let restriction = RestrictionLayer::new(
        PolicyTier::Project,
        vec![RestrictionRule::require_approval(
            Sha256Digest::new([226; 32]),
            ScopeSelector::new(
                ActorSelector::any_within_parent(),
                RoleSelector::any_within_parent(),
                EnvironmentSelector::any_within_parent(),
                PermissionSelector::any_within_parent(),
                revision,
            ),
            requirement,
        )],
    )
    .map_err(super::debug_error)?;
    let policy = PolicyDefinition::new(revision.policy_id(), ceiling, registry, vec![restriction])
        .map_err(super::debug_error)?;
    let scope = CapabilityScope::new(
        actor_id,
        ActorRole::Writer,
        environment,
        permissions()?,
        revision,
        window(epoch, 0, 60_000)?,
        UseLimit::limited(1).map_err(super::debug_error)?,
    );
    let instant = AuthorityInstant::new(Generation::new(epoch).map_err(super::debug_error)?, 0);
    let decision = policy
        .evaluate(AuthorizationRequest::new(scope), AuthorityTimeState::new(instant), instant)
        .map_err(super::debug_error)?;
    let challenge = decision
        .into_parts()
        .1
        .ok_or_else(|| io::Error::other("approval fixture policy did not require approval"))?;
    ApprovalRequest::new(
        ApprovalRequestId::new([227; 16]).map_err(super::debug_error)?,
        ActionId::new([228; 16]).map_err(super::debug_error)?,
        ActionDigest::from_sha256(Sha256Digest::new([229; 32])),
        actor_id,
        ActorRole::Writer,
        challenge,
        Sha256Digest::new([230; 32]),
        ParticipantSet::producing(Vec::new()).map_err(super::debug_error)?,
        ParticipantSet::review(Vec::new()).map_err(super::debug_error)?,
        window(epoch, 0, 60_000)?,
    )
    .map_err(super::debug_error)
}

fn permissions() -> io::Result<PermissionSet> {
    PermissionSet::new(vec![Permission::new(
        ResourceId::new([231; 16]).map_err(super::debug_error)?,
        capability_name()?,
    )])
    .map_err(super::debug_error)
}

fn capability_name() -> io::Result<CapabilityName> {
    CapabilityName::new("workspace.inspect".to_owned()).map_err(super::debug_error)
}

fn window(epoch: u64, start: u64, end: u64) -> io::Result<ValidityWindow> {
    let epoch = Generation::new(epoch).map_err(super::debug_error)?;
    ValidityWindow::new(AuthorityInstant::new(epoch, start), AuthorityInstant::new(epoch, end))
        .map_err(super::debug_error)
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new([232; 16]).expect("fixture acceptance identity"),
        HarnessId::new([233; 16]).expect("fixture harness identity"),
        WorkspaceId::new([234; 16]).expect("fixture workspace identity"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([235; 16]).expect("fixture policy identity"),
        ProviderProfileId::new([236; 16]).expect("fixture provider identity"),
    )
}
