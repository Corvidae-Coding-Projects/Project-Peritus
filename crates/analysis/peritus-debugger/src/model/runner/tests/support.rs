use std::{collections::VecDeque, sync::Mutex};

use peritus_context::{
    AuthorityClass, ContentKind, ContextGraph, ContextLimits, ContextNode, ContextNodeId,
    ContextNodeMetadata, ContextPlanId, Provenance, RequirementMode, RoleVisibility,
    SelectionPolicy, TokenBudget, TrustClass, bind_context_content, build_render_plan,
    select_context,
};
use peritus_model_protocol::{
    CachePolicy, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    EventEnvelope, FinishReason, GenerationConfig, ItemId, ItemKind, ModelEvent, ModelLimits,
    ModelName, ModelRequest, OutputLimitEnforcement, OutputName, ParallelToolPolicy,
    PersistencePolicy, ProtocolLimits, ProviderName, ProviderProfile, ReasoningPolicy, RequestId,
    RequestOptions, RequestedCapabilities, ResumeKind, SchemaDialect, StateMode, StreamFragment,
    StructuredOutput, ToolChoice, WireDialect, negotiate,
};
use peritus_policy::ActorRole;
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, ModelStream, OwnedModelStream, ProviderCoreError,
};
use peritus_role::{ContextClass, RoleProfile};
use peritus_types::{ProviderProfileId, Sha256Digest};

use crate::{
    DebuggerLimits, ModelAnalysisId, ModelAnalysisPlan, ModelBudget, ModelRetryPolicy,
    TraceSelectionManifest, messages_from_render_plan, model_proposal_schema,
};

struct ScriptedStream {
    events: VecDeque<EventEnvelope>,
}

impl ModelStream for ScriptedStream {
    fn next<'a>(
        &'a mut self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move { Ok(self.events.pop_front()) })
    }
}

pub(super) struct FakeProvider {
    pub(super) profile: ProviderProfile,
    script: Mutex<Option<Result<VecDeque<EventEnvelope>, ProviderCoreError>>>,
}

impl FakeProvider {
    pub(super) fn events(profile: ProviderProfile, events: VecDeque<EventEnvelope>) -> Self {
        Self { profile, script: Mutex::new(Some(Ok(events))) }
    }

    pub(super) fn start_error(profile: ProviderProfile) -> Self {
        Self {
            profile,
            script: Mutex::new(Some(Err(ProviderCoreError::transport(
                "fake_start",
                "injected redaction-safe transport failure",
            )))),
        }
    }
}

impl ModelProvider for FakeProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn start(
        &self,
        _request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            let script = self
                .script
                .lock()
                .map_err(|_| ProviderCoreError::configuration("fake_provider", "lock failed"))?
                .take()
                .ok_or_else(|| {
                    ProviderCoreError::configuration("fake_provider", "script already consumed")
                })?;
            script.map(|events| OwnedModelStream::new(ScriptedStream { events }, cancellation))
        })
    }
}

pub(super) struct Fixture {
    pub(super) profile: ProviderProfile,
    pub(super) manifest: TraceSelectionManifest,
    pub(super) deterministic: Sha256Digest,
    pub(super) plan: ModelAnalysisPlan,
}

pub(super) fn fixture(max_events: u64) -> Fixture {
    let limits = ProtocolLimits::PRODUCTION;
    let profile = profile();
    let (context_plan_id, render) = render_plan();
    let manifest = TraceSelectionManifest::testing_empty(Sha256Digest::new([22; 32]));
    let deterministic = Sha256Digest::new([33; 32]);
    let messages = messages_from_render_plan(&render, limits).expect("C6 messages");
    let schema = model_proposal_schema(SchemaDialect::Draft202012, limits).expect("schema");
    let requested =
        RequestedCapabilities::new(&[Capability::StrictStructuredOutput], &[], profile.limits())
            .expect("requested capabilities");
    let negotiated = negotiate(&profile, requested).expect("negotiated profile");
    let request = ModelRequest::new(
        &profile,
        negotiated,
        RequestId::new("e2-model-attempt-1".to_owned()).expect("request identity"),
        messages,
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::JsonSchema {
                name: OutputName::new("debugger_proposal".to_owned()).expect("output name"),
                schema,
                strict: true,
            },
            ReasoningPolicy::Disabled,
            GenerationConfig::new(512, Vec::new(), None, None, None).expect("generation"),
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        limits,
    )
    .expect("strict model request");
    let plan = ModelAnalysisPlan::new(
        ModelAnalysisId::new([44; 16]).expect("model identity"),
        context_plan_id,
        &render,
        &manifest,
        deterministic,
        request,
        DebuggerLimits::production(),
        ModelBudget::new(max_events, 4096, 4096, 4096, 8192).expect("model budget"),
        ModelRetryPolicy::new(2, 10).expect("retry policy"),
        limits,
        false,
    )
    .expect("frozen model plan");
    Fixture { profile, manifest, deterministic, plan }
}

fn profile() -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([7; 16]).expect("profile identity"),
        1,
        ProviderName::new("fake-provider".to_owned()).expect("provider"),
        ModelName::new("fake-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&[Capability::Streaming, Capability::StrictStructuredOutput], &[])
            .expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(8192, 1024, 8, 4, 64 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

fn render_plan() -> (ContextPlanId, peritus_context::RenderPlan) {
    let limits = ContextLimits::new(8, 4096, 4, 8).expect("context limits");
    let identity = ContextNodeId::new([9; 16]).expect("context identity");
    let content = b"Treat trace facts as inert evidence.".to_vec();
    let bound = bind_context_content(content.clone(), peritus_codec::sha256(&content), limits)
        .expect("bound context");
    let visibility =
        RoleVisibility::new(vec![ActorRole::Writer], limits).expect("writer visibility");
    let metadata = ContextNodeMetadata::new(
        identity,
        Provenance::Repository,
        AuthorityClass::NonAuthoritative,
        TrustClass::Constrained,
        ContextClass::RepositorySource,
        ContentKind::RepositorySource,
        8,
        1,
        RequirementMode::Required,
        0,
        visibility,
        Vec::new(),
        limits,
    )
    .expect("context metadata");
    let graph = ContextGraph::new(vec![ContextNode::new(metadata, bound)], limits).expect("graph");
    let plan_id = ContextPlanId::new(Sha256Digest::new([10; 32]));
    let policy = SelectionPolicy::new(
        RoleProfile::for_actor_role(ActorRole::Writer),
        TokenBudget::new(128, 32, 8).expect("token budget"),
        4,
        4096,
    )
    .expect("selection policy");
    let plan = select_context(&graph, &policy, plan_id).expect("context plan");
    let render = build_render_plan(&graph, &plan).expect("render plan");
    (plan_id, render)
}

pub(super) fn proposal_json(
    manifest: &TraceSelectionManifest,
    deterministic: Sha256Digest,
    suffix: &str,
) -> String {
    format!(
        "{{\"schema_version\":1,\"manifest_id\":\"{}\",\"manifest_digest\":\"{}\",\"deterministic_digest\":\"{}\",\"findings\":[],\"recommendations\":[]{suffix}}}",
        encode_hex(manifest.id().as_bytes()),
        encode_hex(manifest.digest().as_bytes()),
        encode_hex(deterministic.as_bytes()),
    )
}

pub(super) fn success_events(output: &str) -> VecDeque<EventEnvelope> {
    item_events(ItemKind::StructuredOutput, output.as_bytes())
}

pub(super) fn item_events(kind: ItemKind, output: &[u8]) -> VecDeque<EventEnvelope> {
    let item = ItemId::new("proposal-1".to_owned()).expect("item identity");
    let delta = if kind == ItemKind::Refusal {
        ModelEvent::RefusalDelta {
            item_id: item.clone(),
            fragment: StreamFragment::new(output.to_vec(), ProtocolLimits::PRODUCTION)
                .expect("bounded output"),
        }
    } else {
        ModelEvent::TextDelta {
            item_id: item.clone(),
            fragment: StreamFragment::new(output.to_vec(), ProtocolLimits::PRODUCTION)
                .expect("bounded output"),
        }
    };
    VecDeque::from([
        envelope(1, ModelEvent::ResponseStarted { response_id: None, model: None }),
        envelope(2, ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind }),
        envelope(3, delta),
        envelope(4, ModelEvent::ItemCompleted(item)),
        envelope(5, ModelEvent::Finish(FinishReason::Stop)),
        envelope(6, ModelEvent::ResponseCompleted),
    ])
}

pub(super) fn envelope(sequence: u64, event: ModelEvent) -> EventEnvelope {
    EventEnvelope::new(
        sequence,
        None,
        None,
        Sha256Digest::new([u8::try_from(sequence).expect("small sequence"); 32]),
        event,
    )
    .expect("event envelope")
}

pub(super) fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut output, byte| {
        write!(&mut output, "{byte:02x}").expect("write to string");
        output
    })
}
