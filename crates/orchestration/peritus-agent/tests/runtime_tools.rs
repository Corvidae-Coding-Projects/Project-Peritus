//! End-to-end D0 → C4 fake-tool execution with committed independent authority.

#[allow(unused_imports, reason = "shared C4 fixture exports a wider test API")]
mod router_fixture;

use peritus_agent::{
    ActivePhase, AgentBinding, AgentCommandKind, AgentDriver, AgentLimits, AgentPhase,
    CompletionProposal, CompletionRequest, ContextRecord, ModelCallId, ModelTerminalRecord,
    ProfileRevision, RuntimeToolSlot, SafeText, TerminalKind, ToolBatchCoordinator,
    ToolDispatchAdvance, ToolInvocationPlan, ToolOrdinal, TranscriptDigests, TransitionIdentity,
};
use peritus_codec::{CodecLimits, encode_frame, sha256};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CapabilityCommitRequest,
    CommittedBudgetTransition, CommittedCapabilityUse, CommittedKernelTransition,
    CommittedLeaseTransition, CurrentAuthorityEpoch, EventDraft, ExactFrame, HeadExpectation,
    LeaseCommitRequest, SqliteJournal,
};
use peritus_leases::{
    AcquireLease, LeaseAggregate, LeaseDuration, LeaseHolder, LeaseScope, LeaseTransition,
    LeaseTransitionOutcome, LeaseUseOutcome, MintLease, UseLease,
};
use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, JsonBounds, ProtocolLimits, ToolCallId, ToolName,
};
use std::sync::Arc;

use peritus_policy::{
    ActorRole, AuthorityInstant, CapabilityUseRequest, OperationClass, OperationDescriptor,
    OperationRegistry, Permission, RiskClass, RiskSet,
};
use peritus_protocol::ActionIntentDto;
use peritus_role::RoleProfile;
use peritus_tool_protocol::{
    BoundedJson, BoundedText, CallLimits, ControlSet, IdempotencyKey, IdempotencySemantics,
    ImplementationIdentity, JsonLimits, LeaseRequirement, ProtocolCompatibility, Schema,
    SchemaProperty, SemanticVersion, SideEffectClass, ToolDescriptor, ToolLimits, ToolResult,
    ToolTiming,
};
use peritus_tool_router::{
    AuthorizedInvocation, DispatchFailure, RouterLimits, ToolAuthorizationRequest, ToolDispatcher,
    ToolRegistry, ToolRouter, ToolStart, tool_action_intent,
};
use peritus_types::{
    CommandId, EventId, EventSequence, Generation, RevisionNumber, RevisionTuple, Sha256Digest,
};

use router_fixture::{Ids, TestRoot};

struct CompletingDispatcher {
    identity: ImplementationIdentity,
    digest: peritus_tool_protocol::SchemaDigest,
    calls: usize,
}

struct AuthorityBundle {
    kernel: CommittedKernelTransition,
    capability: CommittedCapabilityUse,
    budget: CommittedBudgetTransition,
    lease: Option<CommittedLeaseTransition>,
    epoch: CurrentAuthorityEpoch,
    observed_at: AuthorityInstant,
}

impl AuthorityBundle {
    const fn request<'a>(
        &'a self,
        ids: &Ids,
        intent: &'a ActionIntentDto,
        prepared_digest: Sha256Digest,
    ) -> ToolAuthorizationRequest<'a> {
        ToolAuthorizationRequest::new(
            intent,
            &self.kernel,
            &self.capability,
            &self.budget,
            self.lease.as_ref(),
            &self.epoch,
            ids.revision,
            ids.session,
            self.observed_at,
            ids.revision.workspace_generation(),
            ids.revision.workspace_revision(),
            prepared_digest,
        )
    }
}

impl From<router_fixture::AuthorityReceipts> for AuthorityBundle {
    fn from(value: router_fixture::AuthorityReceipts) -> Self {
        Self {
            kernel: value.kernel,
            capability: value.capability,
            budget: value.budget,
            lease: None,
            epoch: value.epoch,
            observed_at: value.observed_at,
        }
    }
}

impl CompletingDispatcher {
    fn new(descriptor: &ToolDescriptor) -> Self {
        Self {
            identity: descriptor.implementation_identity().clone(),
            digest: descriptor.descriptor_digest(),
            calls: 0,
        }
    }
}

impl ToolDispatcher for CompletingDispatcher {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        &self.identity
    }

    fn descriptor_digest(&self) -> peritus_tool_protocol::SchemaDigest {
        self.digest
    }

    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        self.calls += 1;
        let timing =
            ToolTiming::new(invocation.observed_at(), invocation.observed_at()).expect("timing");
        let result = ToolResult::success(
            invocation.prepared(),
            BoundedJson::null(),
            BoundedText::new("inspected workspace".to_owned()).expect("model output"),
            BoundedText::new("inspected workspace".to_owned()).expect("human output"),
            Vec::new(),
            timing,
            router_fixture::complete_truncation(),
            0,
        )
        .expect("result");
        Ok(ToolStart::Completed(result))
    }
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the end-to-end test keeps exact authority, router, driver, and observation order visible"
)]
fn authorized_fake_inspection_runs_once_and_results_return_to_context() {
    let root = TestRoot::new();
    let ids = Ids::new(90);
    let mut journal = router_fixture::open_journal(&root);
    let mut router = router_fixture::router(IdempotencySemantics::ReplayTerminal);
    let completed = CompletedToolCall::new(
        ToolCallId::new("call-inspect-1".to_owned()).expect("call ID"),
        ToolName::new("fixture.inspect".to_owned()).expect("tool name"),
        CanonicalJson::parse(r#"{"count":1}"#, JsonBounds::value(ProtocolLimits::PRODUCTION))
            .expect("arguments"),
    )
    .expect("completed call");
    let plan = ToolInvocationPlan::from_model(
        completed,
        ids.action,
        SemanticVersion::new(1, 0, 0).expect("version"),
        CallLimits::new(1_000, 2_048, 256, 256, 4, 1).expect("call limits"),
        ids.revision,
        AuthorityInstant::new(Generation::first(), 80),
        IdempotencyKey::new("exact".to_owned()).expect("idempotency"),
        JsonLimits::PRODUCTION,
    )
    .expect("plan");
    let prepared = router.prepare(plan.call().clone()).expect("preparation");
    let intent = tool_action_intent(
        &prepared,
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.resource,
    );
    let receipts = router_fixture::commit_authority(&mut journal, &ids, &intent, 1_000, true);
    let exposed = router
        .exposed(ActorRole::ProviderToolWorker, receipts.capability.transition().scope())
        .expect("exposure");
    let coordinator =
        ToolBatchCoordinator::prepare(&router, &exposed, vec![plan], 4, 2).expect("coordinator");
    let proposals = coordinator
        .slots()
        .iter()
        .map(RuntimeToolSlot::agent_proposal)
        .collect::<Result<Vec<_>, _>>()
        .expect("proposals");

    let mut driver = AgentDriver::start(
        &mut journal,
        binding(&ids),
        AgentLimits::new(8, 32, 4, 8_192, 8_192, 2, 64).expect("agent limits"),
        transition(170),
        CodecLimits::PRODUCTION,
    )
    .expect("agent start");
    driver
        .drive_once(
            &mut journal,
            transition(171),
            AgentCommandKind::ContextPrepared(ContextRecord::new(
                digest(20),
                digest(21),
                digest(22),
                digest(23),
                None,
            )),
        )
        .expect("context");
    driver
        .drive_once(
            &mut journal,
            transition(172),
            AgentCommandKind::ModelRequestStarted {
                call_id: ModelCallId::new(digest(24)).expect("model call"),
                request_digest: digest(25),
            },
        )
        .expect("model start");
    driver
        .drive_once(
            &mut journal,
            transition(173),
            AgentCommandKind::ToolCallsProposed {
                terminal: ModelTerminalRecord::new(digest(26), true, false, true),
                proposals,
            },
        )
        .expect("tool proposals");
    driver.attach_prepared_tools(coordinator).expect("attach coordinator");
    driver
        .request_tool_authorization_once(&mut journal, transition(174))
        .expect("authorization phase");
    driver
        .authorize_tool_once(
            &mut journal,
            transition(175),
            ToolOrdinal::new(0),
            receipts.capability.state_digest(),
        )
        .expect("authorized");
    driver.begin_tool_execution_once(&mut journal, transition(176)).expect("begin execution");

    let authorization =
        router_fixture::authority_request(&ids, &intent, &receipts, prepared.prepared_digest());
    let mut dispatcher = CompletingDispatcher::new(prepared.descriptor());
    assert_eq!(
        driver
            .dispatch_tool_once(
                &mut journal,
                transition(177),
                ToolOrdinal::new(0),
                &mut router,
                &authorization,
                &mut dispatcher,
            )
            .expect("dispatch"),
        ToolDispatchAdvance::Terminal,
    );
    driver
        .record_tool_observation_once(
            &mut journal,
            transition(178),
            ToolOrdinal::new(0),
            Vec::new(),
        )
        .expect("record result")
        .expect("terminal observation");
    assert_eq!(dispatcher.calls, 1);
    assert_eq!(
        driver
            .tools()
            .expect("tools")
            .ordered_model_results(ProtocolLimits::PRODUCTION)
            .expect("model results")
            .len(),
        1,
    );
    driver
        .drive_once(&mut journal, transition(179), AgentCommandKind::ResultRecordingStarted)
        .expect("recording");
    driver
        .drive_once(
            &mut journal,
            transition(180),
            AgentCommandKind::ResultsRecorded { transcript_digest: digest(88) },
        )
        .expect("results recorded");
    assert_eq!(driver.state().phase(), AgentPhase::Active(ActivePhase::PreparingContext));
    assert_eq!(driver.state().counters().tool_calls(), 1);
}

#[test]
fn complete_inspect_edit_run_test_loop_reaches_only_a_completion_proposal() {
    let driver_root = TestRoot::new();
    let ids = Ids::new(90);
    let mut journal = router_fixture::open_journal(&driver_root);
    let mut driver = AgentDriver::start(
        &mut journal,
        binding(&ids),
        AgentLimits::new(8, 64, 8, 32_768, 32_768, 2, 64).expect("agent limits"),
        transition(10),
        CodecLimits::PRODUCTION,
    )
    .expect("agent start");

    let cycles = [
        (20, "fixture.inspect", OperationClass::Inspection, SideEffectClass::None),
        (40, "fixture.edit", OperationClass::WorkspaceMutation, SideEffectClass::Workspace),
        (60, "fixture.run", OperationClass::Execution, SideEffectClass::Process),
        (80, "fixture.test", OperationClass::Execution, SideEffectClass::Process),
    ];
    let mut tool_transcript = digest(1);
    for (base, name, operation, side_effect) in cycles {
        tool_transcript =
            execute_cycle(&mut driver, &mut journal, base, name, operation, side_effect);
    }
    assert_eq!(driver.state().counters().tool_calls(), 4);
    assert_eq!(driver.state().phase(), AgentPhase::Active(ActivePhase::PreparingContext));

    let context_digest = digest(100);
    driver
        .drive_once(
            &mut journal,
            transition(100),
            AgentCommandKind::ContextPrepared(ContextRecord::new(
                context_digest,
                digest(101),
                digest(102),
                digest(103),
                None,
            )),
        )
        .expect("final context");
    driver
        .drive_once(
            &mut journal,
            transition(101),
            AgentCommandKind::ModelRequestStarted {
                call_id: ModelCallId::new(digest(104)).expect("model call"),
                request_digest: digest(105),
            },
        )
        .expect("final model");
    let terminal = ModelTerminalRecord::new(digest(106), true, false, true);
    let proposal = CompletionProposal::new(
        SafeText::new("inspected, edited, ran, and tested the workspace".to_owned())
            .expect("summary"),
        Vec::new(),
        vec![SafeText::new("gate acceptance is outside D0".to_owned()).expect("uncertainty")],
        ids.revision,
        TranscriptDigests::new(context_digest, terminal.response_digest(), tool_transcript),
        CompletionRequest::RunGates,
    )
    .expect("proposal");
    driver
        .drive_once(
            &mut journal,
            transition(102),
            AgentCommandKind::CompletionProposed { terminal, proposal },
        )
        .expect("proposal transition");
    assert_eq!(driver.state().phase(), AgentPhase::Active(ActivePhase::ProposedCompletion));
    assert_eq!(driver.state().terminal_kind(), None);
    driver
        .drive_once(&mut journal, transition(103), AgentCommandKind::CompletionCommitted)
        .expect("complete inner turn");
    assert_eq!(driver.state().terminal_kind(), Some(TerminalKind::Completed));
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "the integration helper keeps the complete authority-before-effect cycle explicit"
)]
fn execute_cycle(
    driver: &mut AgentDriver,
    journal: &mut SqliteJournal,
    base: u8,
    name: &str,
    operation: OperationClass,
    side_effect: SideEffectClass,
) -> Sha256Digest {
    let authority_root = TestRoot::new();
    let ids = Ids::new_named(90, name);
    let mut authority_journal = router_fixture::open_journal(&authority_root);
    let mut router = named_router(name, operation, side_effect);
    let completed = CompletedToolCall::new(
        ToolCallId::new(format!("call-{name}")).expect("call ID"),
        ToolName::new(name.to_owned()).expect("tool name"),
        CanonicalJson::parse(r#"{"count":1}"#, JsonBounds::value(ProtocolLimits::PRODUCTION))
            .expect("arguments"),
    )
    .expect("completed call");
    let plan = ToolInvocationPlan::from_model(
        completed,
        ids.action,
        SemanticVersion::new(1, 0, 0).expect("version"),
        CallLimits::new(1_000, 2_048, 256, 256, 4, 1).expect("call limits"),
        ids.revision,
        AuthorityInstant::new(Generation::first(), 80),
        IdempotencyKey::new(format!("key-{name}")).expect("idempotency"),
        JsonLimits::PRODUCTION,
    )
    .expect("plan");
    let prepared = router.prepare(plan.call().clone()).expect("prepare");
    let intent = tool_action_intent(
        &prepared,
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.resource,
    );
    let mut receipts = AuthorityBundle::from(router_fixture::commit_authority(
        &mut authority_journal,
        &ids,
        &intent,
        1_000,
        true,
    ));
    if side_effect == SideEffectClass::Workspace {
        receipts = add_workspace_lease(&mut authority_journal, &ids, &intent, receipts);
    }
    let exposed = router
        .exposed(ActorRole::ProviderToolWorker, receipts.capability.transition().scope())
        .expect("exposure");
    let coordinator =
        ToolBatchCoordinator::prepare(&router, &exposed, vec![plan], 4, 1).expect("coordinator");
    let proposals = coordinator
        .slots()
        .iter()
        .map(RuntimeToolSlot::agent_proposal)
        .collect::<Result<Vec<_>, _>>()
        .expect("proposals");

    driver
        .drive_once(
            journal,
            transition(base),
            AgentCommandKind::ContextPrepared(ContextRecord::new(
                digest(base),
                digest(base.wrapping_add(1)),
                digest(base.wrapping_add(2)),
                digest(base.wrapping_add(3)),
                None,
            )),
        )
        .expect("context");
    driver
        .drive_once(
            journal,
            transition(base.wrapping_add(1)),
            AgentCommandKind::ModelRequestStarted {
                call_id: ModelCallId::new(digest(base.wrapping_add(4))).expect("model call"),
                request_digest: digest(base.wrapping_add(5)),
            },
        )
        .expect("model start");
    driver
        .drive_once(
            journal,
            transition(base.wrapping_add(2)),
            AgentCommandKind::ToolCallsProposed {
                terminal: ModelTerminalRecord::new(digest(base.wrapping_add(6)), true, false, true),
                proposals,
            },
        )
        .expect("proposals");
    driver.attach_prepared_tools(coordinator).expect("attach");
    driver
        .request_tool_authorization_once(journal, transition(base.wrapping_add(3)))
        .expect("authorization phase");
    driver
        .authorize_tool_once(
            journal,
            transition(base.wrapping_add(4)),
            ToolOrdinal::new(0),
            receipts.capability.state_digest(),
        )
        .expect("authorize");
    driver.begin_tool_execution_once(journal, transition(base.wrapping_add(5))).expect("execution");
    let authorization = receipts.request(&ids, &intent, prepared.prepared_digest());
    let mut dispatcher = CompletingDispatcher::new(prepared.descriptor());
    assert_eq!(
        driver
            .dispatch_tool_once(
                journal,
                transition(base.wrapping_add(6)),
                ToolOrdinal::new(0),
                &mut router,
                &authorization,
                &mut dispatcher,
            )
            .expect("dispatch"),
        ToolDispatchAdvance::Terminal,
    );
    driver
        .record_tool_observation_once(
            journal,
            transition(base.wrapping_add(7)),
            ToolOrdinal::new(0),
            Vec::new(),
        )
        .expect("observation")
        .expect("terminal");
    assert_eq!(dispatcher.calls, 1);
    driver
        .drive_once(
            journal,
            transition(base.wrapping_add(8)),
            AgentCommandKind::ResultRecordingStarted,
        )
        .expect("recording");
    let transcript = digest(base.wrapping_add(10));
    driver
        .drive_once(
            journal,
            transition(base.wrapping_add(9)),
            AgentCommandKind::ResultsRecorded { transcript_digest: transcript },
        )
        .expect("results");
    driver.clear_runtime_resources();
    transcript
}

fn named_router(
    name: &str,
    operation_class: OperationClass,
    side_effect: SideEffectClass,
) -> ToolRouter {
    let capability = peritus_types::CapabilityName::new(name.to_owned()).expect("capability");
    let risk = match operation_class {
        OperationClass::Inspection => RiskClass::Read,
        OperationClass::WorkspaceMutation => RiskClass::ScopedWrite,
        OperationClass::Execution => RiskClass::Execution,
        _ => panic!("test operation is unsupported"),
    };
    let operation = OperationDescriptor::new(
        capability.clone(),
        operation_class,
        RiskSet::new(vec![risk]).expect("risks"),
    )
    .expect("operation");
    let registry_operation = OperationDescriptor::new(
        capability.clone(),
        operation_class,
        RiskSet::new(vec![risk]).expect("risks"),
    )
    .expect("registry operation");
    let schema = Schema::object(
        vec![
            SchemaProperty::new(
                "count".to_owned(),
                Schema::integer(Some(0), Some(9)).expect("integer"),
                true,
            )
            .expect("property"),
        ],
        false,
    )
    .expect("schema");
    let descriptor = Arc::new(
        ToolDescriptor::new(
            capability,
            SemanticVersion::new(1, 0, 0).expect("version"),
            schema,
            operation,
            side_effect,
            if side_effect == SideEffectClass::Workspace {
                LeaseRequirement::Required
            } else {
                LeaseRequirement::None
            },
            IdempotencySemantics::ReplayTerminal,
            ImplementationIdentity::new(format!("fake:{name}:v1")).expect("implementation"),
            ToolLimits::new(2_000, 4_096, 512, 512, 8, 2, 128).expect("limits"),
            ControlSet::new(false, false, false, true, true),
            ProtocolCompatibility::V1,
            BoundedText::new(format!("fake {name}")).expect("description"),
        )
        .expect("descriptor"),
    );
    let operations = OperationRegistry::new(vec![registry_operation]).expect("operations");
    ToolRouter::new(
        ToolRegistry::new(vec![descriptor], &operations).expect("registry"),
        RouterLimits::new(2, 8).expect("router limits"),
    )
}

fn add_workspace_lease(
    journal: &mut SqliteJournal,
    ids: &Ids,
    intent: &ActionIntentDto,
    receipts: AuthorityBundle,
) -> AuthorityBundle {
    let AuthorityBundle { kernel, capability, budget, lease: _, epoch, observed_at } = receipts;
    let transition_digest = capability.transition().transition_digest();
    let (_, capability) = capability.into_parts();
    let action_digest = intent.digest(CodecLimits::PRODUCTION).expect("action digest");
    let use_request = CapabilityUseRequest::new(
        ids.action,
        action_digest,
        Permission::new(ids.resource, ids.capability.clone()),
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.revision,
        observed_at,
    );
    let capability_use =
        capability.try_use(use_request, transition_digest).expect("second exact capability use");

    let lease_key = aggregate(AggregateKind::Lease, 40);
    let scope = LeaseScope::new(ids.revision.workspace_id(), ids.resource, ids.environment);
    let mint =
        LeaseAggregate::mint(MintLease::new(command(40), scope, instant(10))).expect("mint lease");
    let minted = commit_lease(journal, ids.revision, lease_key, mint, 1, 40, None);
    let active = accepted(minted.into_parts().1.acquire(AcquireLease::new(
        command(41),
        LeaseHolder::new(ids.actor, ids.session),
        LeaseDuration::new(50).expect("lease duration"),
        instant(10),
    )));
    let acquired = commit_lease(journal, ids.revision, lease_key, active, 2, 41, Some(event(40)));
    let active = acquired.into_parts().1;
    let claim = active.active().expect("active lease").claim();
    let logical = match active.authorize_use(UseLease::new(
        command(42),
        claim,
        observed_at,
        capability_use,
    )) {
        LeaseUseOutcome::Accepted(value) => value,
        LeaseUseOutcome::Rejected(failure) => panic!("lease use: {:?}", failure.error()),
    };
    let (lease_transition, capability_transition) = logical.into_parts();
    let capability_key = aggregate(AggregateKind::Approval, 70);
    let capability_head = journal.head(capability_key).expect("capability head").expect("head");
    let capability = journal
        .commit_capability_use(
            CapabilityCommitRequest::new(
                append(
                    journal,
                    capability_key,
                    command(44),
                    2,
                    event(44),
                    Some(event(43)),
                    HeadExpectation::Present(capability_head),
                    ids.revision,
                ),
                capability_transition,
                Some(1),
            )
            .expect("capability request"),
        )
        .expect("capability commit");
    let lease =
        commit_lease(journal, ids.revision, lease_key, lease_transition, 3, 42, Some(event(41)));
    AuthorityBundle { kernel, capability, budget, lease: Some(lease), epoch, observed_at }
}

#[allow(clippy::too_many_arguments, reason = "journal fixture keeps exact event fences explicit")]
fn commit_lease(
    journal: &mut SqliteJournal,
    revision: RevisionTuple,
    key: AggregateKey,
    transition: LeaseTransition,
    sequence: u64,
    seed: u8,
    previous: Option<EventId>,
) -> CommittedLeaseTransition {
    let head = journal
        .head(key)
        .expect("lease head")
        .map_or(HeadExpectation::Absent(key), HeadExpectation::Present);
    journal
        .commit_lease_transition(
            LeaseCommitRequest::new(
                append(
                    journal,
                    key,
                    command(seed),
                    sequence,
                    event(seed),
                    previous,
                    head,
                    revision,
                ),
                transition,
            )
            .expect("lease request"),
        )
        .expect("lease commit")
}

#[allow(clippy::too_many_arguments, reason = "journal fixture keeps exact event fences explicit")]
fn append(
    journal: &SqliteJournal,
    key: AggregateKey,
    command_id: CommandId,
    sequence: u64,
    event_id: EventId,
    previous_event_id: Option<EventId>,
    head: HeadExpectation,
    revision: RevisionTuple,
) -> AppendRequest {
    let frame = ExactFrame::new(
        encode_frame(
            300,
            1,
            &[u8::try_from(sequence).expect("short sequence")],
            CodecLimits::PRODUCTION,
        )
        .expect("frame"),
    )
    .expect("exact frame");
    let draft = EventDraft::new(
        key,
        EventSequence::new(sequence).expect("sequence"),
        event_id,
        previous_event_id,
        frame,
        peritus_evidence::revision_digest(&revision),
        Vec::new(),
    )
    .expect("event draft");
    AppendRequest::new(
        journal.store_id(),
        command_id,
        sha256(command_id.as_bytes()),
        vec![head],
        vec![draft],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
}

fn accepted(outcome: LeaseTransitionOutcome) -> LeaseTransition {
    match outcome {
        LeaseTransitionOutcome::Accepted(value) => value,
        LeaseTransitionOutcome::Rejected(failure) => {
            panic!("lease transition: {:?}", failure.error())
        }
    }
}

fn aggregate(kind: AggregateKind, seed: u8) -> AggregateKey {
    AggregateKey::new(kind, AggregateId::new([seed; 16]).expect("aggregate ID"))
}

fn command(seed: u8) -> CommandId {
    CommandId::new([seed; 16]).expect("command ID")
}

fn event(seed: u8) -> EventId {
    EventId::new([seed; 16]).expect("event ID")
}

const fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}

fn binding(ids: &Ids) -> AgentBinding {
    let role = ActorRole::Writer;
    AgentBinding::new(
        ids.turn,
        ids.attempt,
        ids.actor,
        role,
        RoleProfile::for_actor_role(role),
        ids.session,
        ids.environment,
        ids.revision,
        ids.revision.provider_profile_id(),
        ProfileRevision::new(1).expect("profile revision"),
        RevisionNumber::first(),
    )
    .expect("binding")
}

fn transition(seed: u8) -> TransitionIdentity {
    TransitionIdentity::new(
        CommandId::new([seed; 16]).expect("command ID"),
        EventId::new([seed.wrapping_add(30); 16]).expect("event ID"),
    )
}

const fn digest(seed: u8) -> Sha256Digest {
    Sha256Digest::new([seed; 32])
}
