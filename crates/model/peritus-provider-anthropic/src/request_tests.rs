//! Checked Anthropic request projection tests.

use peritus_model_protocol::{
    BoundedText, CachePolicy, CanonicalJson, Capability, CompletedToolCall, ContentBlock,
    GenerationConfig, JsonBounds, JsonSchema, MediaInput, MediaKind, MediaReferenceKind, MediaType,
    Message, OutputName, ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ReasoningEffort,
    ReasoningPolicy, ReasoningReplay, RequestId, RequestOptions, RequestedCapabilities, Role,
    SchemaDialect, StructuredOutput, SummaryPolicy, ToolCallId, ToolChoice, ToolDefinition,
    ToolName, ToolResult, negotiate,
};
use peritus_provider_core::ProviderCoreErrorKind;

use super::encode;
use crate::test_support::{config, profile, request};

#[test]
fn minimal_request_matches_the_checked_golden_wire_shape() {
    let profile = profile();
    let request = request(&profile, true);
    let encoded = encode(&request, &config(1, Vec::new())).expect("encode");
    let expected_bytes = crate::test_support::fixture("golden_request.json");
    let actual: serde_json::Value = serde_json::from_slice(&encoded).expect("actual JSON");
    let expected: serde_json::Value = serde_json::from_slice(&expected_bytes).expect("golden JSON");
    assert_eq!(actual, expected);
    assert!(!format!("{request:?}").contains("sk-ant"));
}

#[test]
fn forced_streaming_requires_exact_negotiation() {
    let profile = profile();
    let request = request(&profile, false);
    let error = encode(&request, &config(1, Vec::new())).expect_err("streaming was not selected");
    assert_eq!(error.kind(), ProviderCoreErrorKind::InvalidRequest);
}

#[test]
fn system_instructions_are_projected_outside_messages() {
    let profile = profile();
    let negotiated = negotiate(
        &profile,
        RequestedCapabilities::new(
            &[Capability::Streaming, Capability::PromptCaching, Capability::SamplingControls],
            &[],
            profile.limits(),
        )
        .expect("requested capabilities"),
    )
    .expect("negotiation");
    let text =
        |value: &str| BoundedText::new(value.to_owned(), ProtocolLimits::PRODUCTION).expect("text");
    let messages = vec![
        Message::new(
            Role::System,
            vec![ContentBlock::Text(text("keep-private-system"))],
            ProtocolLimits::PRODUCTION,
        )
        .expect("system"),
        Message::new(
            Role::User,
            vec![ContentBlock::Text(text("question"))],
            ProtocolLimits::PRODUCTION,
        )
        .expect("user"),
    ];
    let request = peritus_model_protocol::ModelRequest::new(
        &profile,
        negotiated,
        RequestId::new("request-system".to_owned()).expect("request ID"),
        messages,
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(64, Vec::new(), None, Some(250_000), Some(900_000))
                .expect("generation"),
            CachePolicy::Ephemeral { ttl_seconds: 300 },
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        ProtocolLimits::PRODUCTION,
    )
    .expect("request");
    let encoded = encode(&request, &config(1, Vec::new())).expect("encode");
    let value: serde_json::Value = serde_json::from_slice(&encoded).expect("JSON");
    assert_eq!(value["system"][0]["text"], "keep-private-system");
    assert_eq!(value["messages"][0]["role"], "user");
    assert_eq!(value["messages"].as_array().expect("messages").len(), 1);
    assert_eq!(value["temperature"], 0.25);
    assert_eq!(value["top_p"], 0.9);
    assert_eq!(value["messages"][0]["content"][0]["cache_control"]["ttl"], "5m");
}

#[test]
#[allow(clippy::too_many_lines, reason = "one complete Anthropic wire projection fixture")]
fn rich_tool_media_schema_and_thinking_replay_project_without_semantic_loss() {
    let profile = profile();
    let negotiated = negotiate(
        &profile,
        RequestedCapabilities::new(
            &[
                Capability::Streaming,
                Capability::ToolCalls,
                Capability::StrictStructuredOutput,
                Capability::ImageInput,
                Capability::DocumentInput,
                Capability::ReasoningControls,
            ],
            &[],
            profile.limits(),
        )
        .expect("capabilities"),
    )
    .expect("negotiation");
    let limits = ProtocolLimits::PRODUCTION;
    let schema = JsonSchema::parse(
        r#"{"additionalProperties":false,"properties":{"city":{"type":"string"}},"required":["city"],"type":"object"}"#,
        SchemaDialect::Draft202012,
        JsonBounds::schema(limits),
    )
    .expect("schema");
    let tool_name = ToolName::new("weather".to_owned()).expect("tool name");
    let call_id = ToolCallId::new("toolu_replay".to_owned()).expect("call ID");
    let arguments =
        CanonicalJson::parse(r#"{"city":"Paris"}"#, JsonBounds::value(limits)).expect("arguments");
    let call =
        CompletedToolCall::new(call_id.clone(), tool_name.clone(), arguments).expect("tool call");
    let replay = ReasoningReplay::new(
        Some(BoundedText::new("checked".to_owned(), limits).expect("reasoning")),
        br#"{"signature":"opaque-signature","type":"thinking"}"#.to_vec(),
        limits,
    )
    .expect("replay");
    let image = MediaInput::inline(
        MediaKind::Image,
        MediaType::new("image/png".to_owned()).expect("media type"),
        vec![1, 2, 3],
        limits,
    )
    .expect("image");
    let document = MediaInput::referenced(
        MediaKind::Document,
        MediaType::new("application/pdf".to_owned()).expect("media type"),
        MediaReferenceKind::HttpsUrl,
        "https://example.invalid/report.pdf".to_owned(),
        None,
    )
    .expect("document");
    let messages = vec![
        Message::new(
            Role::User,
            vec![ContentBlock::Image(image), ContentBlock::Document(document)],
            limits,
        )
        .expect("user"),
        Message::new(
            Role::Assistant,
            vec![ContentBlock::Reasoning(replay), ContentBlock::ToolCall(call)],
            limits,
        )
        .expect("assistant"),
        Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult(ToolResult::new(
                call_id,
                CanonicalJson::parse(r#"{"temperature":21}"#, JsonBounds::value(limits))
                    .expect("result"),
                false,
            ))],
            limits,
        )
        .expect("tool result"),
    ];
    let request = peritus_model_protocol::ModelRequest::new(
        &profile,
        negotiated,
        RequestId::new("request-rich".to_owned()).expect("request ID"),
        messages,
        vec![ToolDefinition::new(
            tool_name.clone(),
            Some(BoundedText::new("Read weather".to_owned(), limits).expect("description")),
            schema.clone(),
            true,
        )],
        ToolChoice::Specific(tool_name),
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::JsonSchema {
                name: OutputName::new("weather_result".to_owned()).expect("output name"),
                schema,
                strict: true,
            },
            ReasoningPolicy::Effort { effort: ReasoningEffort::High, summary: SummaryPolicy::None },
            GenerationConfig::new(512, Vec::new(), None, None, None).expect("generation"),
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        limits,
    )
    .expect("request");
    let encoded = encode(&request, &config(1, Vec::new())).expect("encode");
    let wire: serde_json::Value = serde_json::from_slice(&encoded).expect("wire JSON");
    assert_eq!(wire["messages"][0]["content"][0]["source"]["type"], "base64");
    assert_eq!(wire["messages"][0]["content"][1]["source"]["type"], "url");
    assert_eq!(wire["messages"][1]["content"][0]["signature"], "opaque-signature");
    assert_eq!(wire["messages"][1]["content"][1]["type"], "tool_use");
    assert_eq!(wire["messages"][2]["content"][0]["type"], "tool_result");
    assert_eq!(wire["tools"][0]["input_schema"]["additionalProperties"], false);
    assert_eq!(wire["tools"][0]["strict"], true);
    assert_eq!(wire["output_config"]["format"]["type"], "json_schema");
    assert_eq!(wire["output_config"]["effort"], "high");
}
