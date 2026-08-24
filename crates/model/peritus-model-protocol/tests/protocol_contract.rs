//! End-to-end protocol request, canonical identity, reducer, and redaction contracts.

use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, CanonicalJson, Capability, CapabilityMatrix,
    CapabilityProvenance, CompletedToolCall, ContentBlock, EventEnvelope, EventId, FinishReason,
    IdempotencyKey, ItemId, ItemKind, JsonBounds, JsonSchema, MediaInput, MediaKind, MediaType,
    Message, ModelEvent, ModelLimits, ModelName, ModelRequest, OutputLimitEnforcement, OutputName,
    ParallelToolPolicy, PersistencePolicy, ProtocolErrorKind, ProtocolLimits, ProviderExtension,
    ProviderName, ProviderProfile, ReasoningEffort, ReasoningPolicy, ReasoningReplay, ReducedItem,
    ReducerTransition, RequestId, RequestOptions, ResponseId, ResponseReducer, ResumeKind, Role,
    SchemaDialect, StateMode, StreamFragment, StructuredOutput, SummaryPolicy, TerminalOutcome,
    ToolCallId, ToolChoice, ToolDefinition, ToolName, ToolResult, UsageCounters, UsageObservation,
    UsageScope, WireDialect, negotiate,
};
use peritus_types::ProviderProfileId;

fn text(value: &str) -> BoundedText {
    BoundedText::new(value.to_owned(), ProtocolLimits::PRODUCTION).expect("bounded text")
}

fn all_capabilities() -> Vec<Capability> {
    vec![
        Capability::Streaming,
        Capability::ToolCalls,
        Capability::ParallelToolCalls,
        Capability::StrictStructuredOutput,
        Capability::PromptCaching,
        Capability::ImageInput,
        Capability::AudioInput,
        Capability::DocumentInput,
        Capability::ReasoningControls,
        Capability::ReasoningSummaries,
        Capability::ResumableResponse,
        Capability::ConfirmedCancellation,
        Capability::UsageDetail,
        Capability::RateLimitDetail,
        Capability::StoredState,
        Capability::ProviderExtensions,
        Capability::SamplingControls,
    ]
}

fn profile(revision: u64) -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([7; 16]).expect("profile ID"),
        revision,
        ProviderName::new("fixture-provider".to_owned()).expect("provider"),
        ModelName::new("fixture-model-2026-08".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&all_capabilities(), &[]).expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(100_000, 16_384, 64, 8, 4 * 1024 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::BackgroundResumable,
        ResumeKind::ExactCursor,
        CancellationKind::Confirmed,
    )
    .expect("profile")
}

#[allow(
    clippy::too_many_lines,
    reason = "representative fixture intentionally assembles every independent request feature"
)]
fn request(profile: &ProviderProfile, request_id: &str) -> ModelRequest {
    let limits = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        profile,
        peritus_model_protocol::RequestedCapabilities::new(
            &all_capabilities(),
            &[],
            ModelLimits::new(80_000, 8_192, 32, 4, 2 * 1024 * 1024).expect("requested limits"),
        )
        .expect("requested capabilities"),
    )
    .expect("negotiation");
    let schema = JsonSchema::parse(
        r#"{"additionalProperties":false,"properties":{"path":{"type":"string"}},"required":["path"],"type":"object"}"#,
        SchemaDialect::Draft202012,
        JsonBounds::schema(limits),
    )
    .expect("schema");
    let tool_name = ToolName::new("read_file".to_owned()).expect("tool name");
    let tool =
        ToolDefinition::new(tool_name.clone(), Some(text("Read one file")), schema.clone(), true);
    let arguments = CanonicalJson::parse(r#"{"path":"README.md"}"#, JsonBounds::value(limits))
        .expect("arguments");
    let call_id = ToolCallId::new("call-sensitive-1".to_owned()).expect("call ID");
    let call = CompletedToolCall::new(call_id.clone(), tool_name.clone(), arguments)
        .expect("completed call");
    let result = ToolResult::new(
        call_id,
        CanonicalJson::parse(r#"{"content":"fixture"}"#, JsonBounds::value(limits))
            .expect("result"),
        false,
    );
    let image = MediaInput::inline(
        MediaKind::Image,
        MediaType::new("image/png".to_owned()).expect("media type"),
        vec![1, 2, 3, 4],
        limits,
    )
    .expect("inline image");
    let reasoning = ReasoningReplay::new(Some(text("Reviewed the request")), vec![9, 8, 7], limits)
        .expect("replay");
    let messages = vec![
        Message::new(Role::System, vec![ContentBlock::Text(text("Be exact"))], limits)
            .expect("system"),
        Message::new(
            Role::User,
            vec![ContentBlock::Text(text("Inspect this")), ContentBlock::Image(image)],
            limits,
        )
        .expect("user"),
        Message::new(
            Role::Assistant,
            vec![ContentBlock::Reasoning(reasoning), ContentBlock::ToolCall(call)],
            limits,
        )
        .expect("assistant"),
        Message::new(Role::Tool, vec![ContentBlock::ToolResult(result)], limits).expect("tool"),
    ];
    let extension = ProviderExtension::new(
        peritus_model_protocol::ExtensionName::new("fixture.mode".to_owned()).expect("extension"),
        CanonicalJson::parse(r#"{"enabled":true}"#, JsonBounds::value(limits))
            .expect("extension value"),
    );
    let generation = peritus_model_protocol::GenerationConfig::new(
        4_096,
        vec![text("<END>")],
        Some(42),
        Some(250_000),
        Some(900_000),
    )
    .expect("generation");
    let options = RequestOptions::new(
        StructuredOutput::JsonSchema {
            name: OutputName::new("answer".to_owned()).expect("output name"),
            schema,
            strict: true,
        },
        ReasoningPolicy::Effort { effort: ReasoningEffort::High, summary: SummaryPolicy::Concise },
        generation,
        CachePolicy::Automatic,
        PersistencePolicy::new(true, true).expect("persistence"),
        Some(
            peritus_model_protocol::Continuation::new(
                ResponseId::new("response-sensitive".to_owned()).expect("response ID"),
                Some(EventId::new("event-sensitive".to_owned()).expect("event ID")),
                Some(11),
            )
            .expect("continuation"),
        ),
        vec![extension],
    );
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(request_id.to_owned()).expect("request ID"),
        messages,
        vec![tool],
        ToolChoice::Specific(tool_name),
        ParallelToolPolicy::Allowed(4),
        options,
        limits,
    )
    .expect("model request")
}

#[test]
fn canonical_request_binds_semantics_and_excludes_observability_identity() {
    let base_profile = profile(3);
    let first = request(&base_profile, "request-a-sensitive");
    let second = request(&base_profile, "request-b-sensitive");
    assert_eq!(
        first.canonical_bytes().expect("canonical"),
        second.canonical_bytes().expect("canonical")
    );
    assert_eq!(first.idempotency_key().expect("key"), second.idempotency_key().expect("key"));
    let revised = request(&profile(4), "request-a-sensitive");
    assert_ne!(
        first.canonical_bytes().expect("canonical"),
        revised.canonical_bytes().expect("canonical")
    );
    assert!(first.canonical_bytes().expect("canonical").starts_with(b"P5MR"));
}

fn envelope(sequence: u64, identity: &str, raw: &[u8], event: ModelEvent) -> EventEnvelope {
    EventEnvelope::new(
        sequence,
        Some(sequence),
        Some(EventId::new(identity.to_owned()).expect("event ID")),
        peritus_codec::sha256(raw),
        event,
    )
    .expect("event envelope")
}

#[test]
fn reducer_accepts_fragmented_utf8_and_ignores_only_exact_duplicates() {
    let limits = ProtocolLimits::PRODUCTION;
    let mut reducer =
        ResponseReducer::new(ProviderName::new("fixture".to_owned()).expect("provider"), limits);
    let response = ResponseId::new("response-1-sensitive".to_owned()).expect("response");
    let item = ItemId::new("item-1-sensitive".to_owned()).expect("item");
    reducer
        .push(envelope(
            1,
            "event-1",
            b"start",
            ModelEvent::ResponseStarted { response_id: Some(response), model: None },
        ))
        .expect("start");
    reducer
        .push(envelope(
            2,
            "event-2",
            b"item",
            ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::Message },
        ))
        .expect("item");
    let first_delta = envelope(
        3,
        "event-3",
        b"delta-one",
        ModelEvent::TextDelta {
            item_id: item.clone(),
            fragment: StreamFragment::new(vec![b'h', 0xc3], limits).expect("fragment"),
        },
    );
    reducer.push(first_delta.clone()).expect("first delta");
    assert_eq!(reducer.push(first_delta).expect("duplicate"), ReducerTransition::DuplicateIgnored);
    reducer
        .push(envelope(
            4,
            "event-4",
            b"delta-two",
            ModelEvent::TextDelta {
                item_id: item.clone(),
                fragment: StreamFragment::new(vec![0xa9], limits).expect("fragment"),
            },
        ))
        .expect("second delta");
    reducer
        .push(envelope(5, "event-5", b"done", ModelEvent::ItemCompleted(item)))
        .expect("item done");
    reducer
        .push(envelope(
            6,
            "event-6",
            b"usage",
            ModelEvent::Usage(UsageObservation::new(
                UsageScope::Final,
                UsageCounters::new(Some(2), None, None, Some(1), None, None, Some(3), None),
                None,
            )),
        ))
        .expect("usage");
    reducer
        .push(envelope(7, "event-7", b"finish", ModelEvent::Finish(FinishReason::Stop)))
        .expect("finish");
    let terminal = reducer
        .push(envelope(8, "event-8", b"complete", ModelEvent::ResponseCompleted))
        .expect("complete");
    assert!(matches!(terminal, ReducerTransition::Terminal(TerminalOutcome::Succeeded { .. })));
    assert_eq!(
        reducer.terminal().expect("terminal").protocol(),
        peritus_model_protocol::ProtocolVersion::V1
    );
    match &reducer.completed_items()[0] {
        ReducedItem::Text { index, text, .. } => {
            assert_eq!(*index, 0);
            assert_eq!(text.expose_for_wire(), "hé");
        }
        other => panic!("unexpected item: {other:?}"),
    }
}

#[test]
fn reducer_never_turns_conflict_or_eof_into_success() {
    let limits = ProtocolLimits::PRODUCTION;
    let provider = ProviderName::new("fixture".to_owned()).expect("provider");
    let mut conflicting = ResponseReducer::new(provider.clone(), limits);
    let start = envelope(
        1,
        "same-event",
        b"one",
        ModelEvent::ResponseStarted { response_id: None, model: None },
    );
    conflicting.push(start).expect("start");
    let error = conflicting
        .push(envelope(1, "same-event", b"changed", ModelEvent::Heartbeat))
        .expect_err("conflicting duplicate");
    assert_eq!(error.kind(), ProtocolErrorKind::InvalidEvent);
    assert!(matches!(conflicting.terminal(), Some(TerminalOutcome::Failed(_))));

    let mut interrupted = ResponseReducer::new(provider, limits);
    interrupted
        .push(envelope(
            1,
            "start-event",
            b"start",
            ModelEvent::ResponseStarted { response_id: None, model: None },
        ))
        .expect("start");
    assert_eq!(
        interrupted.finish_eof().expect_err("missing terminal").kind(),
        ProtocolErrorKind::IncompleteStream
    );
    assert!(matches!(interrupted.terminal(), Some(TerminalOutcome::Failed(_))));
}

#[test]
fn tool_arguments_are_not_completed_until_closed_and_valid_json() {
    let limits = ProtocolLimits::PRODUCTION;
    let mut reducer =
        ResponseReducer::new(ProviderName::new("fixture".to_owned()).expect("provider"), limits);
    let item = ItemId::new("tool-item".to_owned()).expect("item");
    let call = ToolCallId::new("tool-call".to_owned()).expect("call");
    let events = [
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::ToolCall },
        ModelEvent::ToolCallStarted {
            item_id: item.clone(),
            call_id: call.clone(),
            name: ToolName::new("run".to_owned()).expect("tool"),
        },
        ModelEvent::ToolArgumentDelta {
            call_id: call.clone(),
            fragment: StreamFragment::new(br#"{"value":"#.to_vec(), limits).expect("fragment"),
        },
        ModelEvent::ToolArgumentDelta {
            call_id: call,
            fragment: StreamFragment::new(b"1}".to_vec(), limits).expect("fragment"),
        },
        ModelEvent::ItemCompleted(item),
        ModelEvent::Finish(FinishReason::ToolCalls),
        ModelEvent::ResponseCompleted,
    ];
    for (index, event) in events.into_iter().enumerate() {
        let sequence = u64::try_from(index + 1).expect("sequence");
        reducer
            .push(envelope(
                sequence,
                &format!("tool-event-{sequence}"),
                &[u8::try_from(sequence).expect("byte")],
                event,
            ))
            .expect("tool event");
    }
    assert!(matches!(reducer.terminal(), Some(TerminalOutcome::RequiresAction { .. })));
    assert!(matches!(reducer.completed_items(), [ReducedItem::ToolCall { .. }]));
}

#[test]
fn sensitive_debug_representations_do_not_expose_values() {
    let request = RequestId::new("unique-request-canary".to_owned()).expect("request ID");
    let key = IdempotencyKey::new("unique-key-canary".to_owned()).expect("key");
    let prompt = text("unique-prompt-canary");
    let json = CanonicalJson::parse(
        r#"{"secret":"unique-json-canary"}"#,
        JsonBounds::value(ProtocolLimits::PRODUCTION),
    )
    .expect("JSON");
    let rendered = format!("{request:?} {key:?} {prompt:?} {json:?}");
    for canary in
        ["unique-request-canary", "unique-key-canary", "unique-prompt-canary", "unique-json-canary"]
    {
        assert!(!rendered.contains(canary));
    }
}
