use std::collections::VecDeque;

use peritus_model_protocol::{
    BoundedText, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    EventEnvelope, FailureCategory, FinishReason, ItemId, ItemKind, JsonBounds, JsonSchema,
    ModelEvent, ModelFailure, ModelLimits, ModelName, OutcomeCertainty, OutputLimitEnforcement,
    ProtocolLimits, ProviderName, ProviderProfile, RedactedDiagnostic, ResumeKind, Retryability,
    SchemaDialect, StateMode, StreamFragment, ToolCallId, ToolDefinition, ToolName, TransportPhase,
    WireDialect,
};
use peritus_types::{ProviderProfileId, Sha256Digest};

pub fn profile() -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([0x7A; 16]).expect("profile ID"),
        1,
        ProviderName::new("scripted-provider".to_owned()).expect("provider"),
        ModelName::new("scripted-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&[Capability::ToolCalls], &[]).expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(32_768, 4_096, 16, 1, 256 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

pub fn parallel_profile() -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([0x7B; 16]).expect("profile ID"),
        1,
        ProviderName::new("parallel-provider".to_owned()).expect("provider"),
        ModelName::new("parallel-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(
            &[Capability::ToolCalls, Capability::ParallelToolCalls, Capability::ReasoningControls],
            &[],
        )
        .expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(32_768, 4_096, 16, 4, 256 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

pub fn caching_profile() -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([0x7C; 16]).expect("profile ID"),
        1,
        ProviderName::new("caching-provider".to_owned()).expect("provider"),
        ModelName::new("caching-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&[Capability::ToolCalls, Capability::PromptCaching], &[])
            .expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(32_768, 4_096, 16, 1, 256 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

pub fn read_tool() -> ToolDefinition {
    let limits = ProtocolLimits::PRODUCTION;
    ToolDefinition::new(
        ToolName::new("workspace_read".to_owned()).expect("tool name"),
        Some(BoundedText::new("Read a workspace file".to_owned(), limits).expect("description")),
        JsonSchema::parse(
            r#"{"additionalProperties":false,"properties":{"path":{"type":"string"}},"required":["path"],"type":"object"}"#,
            SchemaDialect::Draft202012,
            JsonBounds::schema(limits),
        )
        .expect("schema"),
        true,
    )
}

pub fn tool_response() -> VecDeque<EventEnvelope> {
    let limits = ProtocolLimits::PRODUCTION;
    let item = ItemId::new("tool-item".to_owned()).expect("item");
    let call = ToolCallId::new("read-call".to_owned()).expect("call");
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::ToolCall },
        ModelEvent::ToolCallStarted {
            item_id: item.clone(),
            call_id: call.clone(),
            name: ToolName::new("workspace_read".to_owned()).expect("tool"),
        },
        ModelEvent::ToolArgumentDelta {
            call_id: call,
            fragment: StreamFragment::new(br#"{"path":"src/lib.rs"}"#.to_vec(), limits)
                .expect("arguments"),
        },
        ModelEvent::ItemCompleted(item),
        ModelEvent::Finish(FinishReason::ToolCalls),
        ModelEvent::ResponseCompleted,
    ])
}

pub fn oversized_tool_argument_response() -> VecDeque<EventEnvelope> {
    let limits = ProtocolLimits::PRODUCTION;
    let item = ItemId::new("oversized-tool-item".to_owned()).expect("item");
    let call = ToolCallId::new("oversized-read-call".to_owned()).expect("call");
    let arguments = format!(r#"{{"path":"{}"}}"#, "x".repeat(120_000));
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::ToolCall },
        ModelEvent::ToolCallStarted {
            item_id: item.clone(),
            call_id: call.clone(),
            name: ToolName::new("workspace_read".to_owned()).expect("tool"),
        },
        ModelEvent::ToolArgumentDelta {
            call_id: call,
            fragment: StreamFragment::new(arguments.into_bytes(), limits).expect("arguments"),
        },
        ModelEvent::ItemCompleted(item),
        ModelEvent::Finish(FinishReason::ToolCalls),
        ModelEvent::ResponseCompleted,
    ])
}

pub fn batch_tool_response() -> VecDeque<EventEnvelope> {
    let limits = ProtocolLimits::PRODUCTION;
    let first_item = ItemId::new("first-tool-item".to_owned()).expect("item");
    let first_call = ToolCallId::new("first-read-call".to_owned()).expect("call");
    let second_item = ItemId::new("second-tool-item".to_owned()).expect("item");
    let second_call = ToolCallId::new("second-read-call".to_owned()).expect("call");
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: first_item.clone(), index: 0, kind: ItemKind::ToolCall },
        ModelEvent::ToolCallStarted {
            item_id: first_item.clone(),
            call_id: first_call.clone(),
            name: ToolName::new("workspace_read".to_owned()).expect("tool"),
        },
        ModelEvent::ToolArgumentDelta {
            call_id: first_call,
            fragment: StreamFragment::new(br#"{"path":"src/lib.rs"}"#.to_vec(), limits)
                .expect("arguments"),
        },
        ModelEvent::ItemCompleted(first_item),
        ModelEvent::ItemStarted {
            item_id: second_item.clone(),
            index: 1,
            kind: ItemKind::ToolCall,
        },
        ModelEvent::ToolCallStarted {
            item_id: second_item.clone(),
            call_id: second_call.clone(),
            name: ToolName::new("workspace_read".to_owned()).expect("tool"),
        },
        ModelEvent::ToolArgumentDelta {
            call_id: second_call,
            fragment: StreamFragment::new(br#"{"path":"src/main.rs"}"#.to_vec(), limits)
                .expect("arguments"),
        },
        ModelEvent::ItemCompleted(second_item),
        ModelEvent::Finish(FinishReason::ToolCalls),
        ModelEvent::ResponseCompleted,
    ])
}

pub fn text_response() -> VecDeque<EventEnvelope> {
    let limits = ProtocolLimits::PRODUCTION;
    let item = ItemId::new("message-item".to_owned()).expect("item");
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::Message },
        ModelEvent::TextDelta {
            item_id: item.clone(),
            fragment: StreamFragment::new(b"implementation inspected".to_vec(), limits)
                .expect("text"),
        },
        ModelEvent::ItemCompleted(item),
        ModelEvent::Finish(FinishReason::Stop),
        ModelEvent::ResponseCompleted,
    ])
}

pub fn recoverable_failure_response() -> VecDeque<EventEnvelope> {
    let failure = ModelFailure::new(
        ProviderName::new("scripted-provider".to_owned()).expect("provider"),
        FailureCategory::MalformedPayload,
        TransportPhase::ReadingBody,
        OutcomeCertainty::MaybeAccepted,
        Retryability::SafeNewRequest,
        None,
        None,
        Some(350),
        RedactedDiagnostic::new("scripted.malformed".to_owned(), None, None, None)
            .expect("diagnostic"),
    );
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ResponseFailed(failure),
    ])
}

pub fn nonretryable_safety_failure_response() -> VecDeque<EventEnvelope> {
    let failure = ModelFailure::new(
        ProviderName::new("scripted-provider".to_owned()).expect("provider"),
        FailureCategory::Safety,
        TransportPhase::Completed,
        OutcomeCertainty::Terminal,
        Retryability::Never,
        None,
        None,
        None,
        RedactedDiagnostic::new("scripted.safety".to_owned(), None, None, None)
            .expect("diagnostic"),
    );
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ResponseFailed(failure),
    ])
}

pub fn empty_response() -> VecDeque<EventEnvelope> {
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::Finish(FinishReason::Stop),
        ModelEvent::ResponseCompleted,
    ])
}

fn response<const N: usize>(events: [ModelEvent; N]) -> VecDeque<EventEnvelope> {
    events
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            let sequence = u64::try_from(index + 1).expect("sequence");
            EventEnvelope::new(
                sequence,
                None,
                None,
                Sha256Digest::new([u8::try_from(index + 1).expect("digest byte"); 32]),
                event,
            )
            .expect("envelope")
        })
        .collect()
}
