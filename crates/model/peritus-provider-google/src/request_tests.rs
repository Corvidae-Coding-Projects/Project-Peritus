//! Stable-v1 request projection tests.

use peritus_model_protocol::{
    BoundedText, CacheKey, CachePolicy, CanonicalJson, Capability, CompletedToolCall, ContentBlock,
    GenerationConfig, JsonBounds, JsonSchema, MediaInput, MediaKind, MediaReferenceKind, MediaType,
    Message, OutputName, ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ReasoningEffort,
    ReasoningPolicy, ReasoningReplay, RequestId, RequestOptions, RequestedCapabilities, Role,
    SchemaDialect, StructuredOutput, SummaryPolicy, ToolCallId, ToolChoice, ToolDefinition,
    ToolName, ToolResult, WireDialect, negotiate,
};
use peritus_provider_core::ProviderCoreErrorKind;

use super::encode;
use crate::test_support::{config, fixture, profile, request};

#[test]
fn minimal_requests_match_both_checked_golden_shapes() {
    for (dialect, fixture_name) in [
        (WireDialect::GeminiInteractionsV1, "golden_interactions_minimal.json"),
        (WireDialect::GeminiGenerateContentV1, "golden_generate_minimal.json"),
    ] {
        let profile = profile(dialect);
        let encoded =
            encode(&request(&profile, true), config(dialect, 1).endpoint()).expect("encode");
        let actual: serde_json::Value = serde_json::from_slice(&encoded.body).expect("actual");
        let expected: serde_json::Value =
            serde_json::from_slice(&fixture(fixture_name)).expect("golden");
        assert_eq!(actual, expected);
        assert!(!encoded.endpoint.as_str().contains("v1beta"));
    }
}

#[test]
fn forced_streaming_requires_exact_negotiation() {
    let profile = profile(WireDialect::GeminiInteractionsV1);
    let result =
        encode(&request(&profile, false), config(WireDialect::GeminiInteractionsV1, 1).endpoint());
    let Err(error) = result else { panic!("streaming not selected") };
    assert_eq!(error.kind(), ProviderCoreErrorKind::InvalidRequest);
}

#[test]
fn system_audio_and_document_inputs_follow_each_dialect_shape() {
    for dialect in [WireDialect::GeminiInteractionsV1, WireDialect::GeminiGenerateContentV1] {
        let profile = profile(dialect);
        let negotiated = negotiate(
            &profile,
            RequestedCapabilities::new(
                &[Capability::Streaming, Capability::AudioInput, Capability::DocumentInput],
                &[],
                profile.limits(),
            )
            .expect("requested"),
        )
        .expect("negotiated");
        let limits = ProtocolLimits::PRODUCTION;
        let audio = MediaInput::inline(
            MediaKind::Audio,
            MediaType::new("audio/wav".to_owned()).expect("type"),
            vec![1, 2, 3],
            limits,
        )
        .expect("audio");
        let document = MediaInput::referenced(
            MediaKind::Document,
            MediaType::new("application/pdf".to_owned()).expect("type"),
            MediaReferenceKind::HttpsUrl,
            "https://example.invalid/report.pdf".to_owned(),
            None,
        )
        .expect("document");
        let text = |value: &str| BoundedText::new(value.to_owned(), limits).expect("text");
        let messages = vec![
            Message::new(Role::System, vec![ContentBlock::Text(text("system"))], limits)
                .expect("system"),
            Message::new(
                Role::User,
                vec![ContentBlock::Audio(audio), ContentBlock::Document(document)],
                limits,
            )
            .expect("user"),
        ];
        let request = peritus_model_protocol::ModelRequest::new(
            &profile,
            negotiated,
            RequestId::new("google-media".to_owned()).expect("request ID"),
            messages,
            Vec::new(),
            ToolChoice::None,
            ParallelToolPolicy::Disabled,
            RequestOptions::new(
                StructuredOutput::Text,
                ReasoningPolicy::Disabled,
                GenerationConfig::new(128, Vec::new(), None, None, None).expect("generation"),
                CachePolicy::Disabled,
                PersistencePolicy::LOCAL_FIRST,
                None,
                Vec::new(),
            ),
            limits,
        )
        .expect("request");
        let wire: serde_json::Value = serde_json::from_slice(
            &encode(&request, config(dialect, 1).endpoint()).expect("encode").body,
        )
        .expect("wire");
        if dialect == WireDialect::GeminiInteractionsV1 {
            assert_eq!(wire["system_instruction"], "system");
            assert_eq!(wire["input"][0]["content"][0]["type"], "audio");
            assert_eq!(wire["input"][0]["content"][1]["type"], "document");
        } else {
            assert_eq!(wire["systemInstruction"]["parts"][0]["text"], "system");
            assert_eq!(wire["contents"][0]["parts"][0]["inlineData"]["data"], "AQID");
            assert_eq!(
                wire["contents"][0]["parts"][1]["fileData"]["fileUri"],
                "https://example.invalid/report.pdf"
            );
        }
    }
}

#[test]
#[allow(clippy::too_many_lines, reason = "one complete dual-dialect semantic projection")]
fn rich_tools_media_schema_cache_and_thought_replay_are_exact() {
    for dialect in [WireDialect::GeminiInteractionsV1, WireDialect::GeminiGenerateContentV1] {
        let profile = profile(dialect);
        let request = rich_request(&profile, dialect);
        let encoded = encode(&request, config(dialect, 1).endpoint()).expect("rich encode");
        let wire: serde_json::Value = serde_json::from_slice(&encoded.body).expect("wire");
        let fixture_name = match dialect {
            WireDialect::GeminiInteractionsV1 => "golden_interactions_realistic.json",
            _ => "golden_generate_realistic.json",
        };
        let expected: serde_json::Value =
            serde_json::from_slice(&fixture(fixture_name)).expect("golden");
        assert_eq!(wire, expected);
    }
}

#[allow(clippy::too_many_lines, reason = "constructs one complete provider-neutral request")]
fn rich_request(
    profile: &peritus_model_protocol::ProviderProfile,
    dialect: WireDialect,
) -> peritus_model_protocol::ModelRequest {
    let limits = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(
            &[
                Capability::Streaming,
                Capability::ToolCalls,
                Capability::StrictStructuredOutput,
                Capability::ImageInput,
                Capability::ReasoningControls,
                Capability::ReasoningSummaries,
                Capability::PromptCaching,
            ],
            &[],
            profile.limits(),
        )
        .expect("requested"),
    )
    .expect("negotiated");
    let schema = JsonSchema::parse(
        r#"{"additionalProperties":false,"properties":{"city":{"type":"string"}},"required":["city"],"type":"object"}"#,
        SchemaDialect::GeminiSubset,
        JsonBounds::schema(limits),
    )
    .expect("schema");
    let name = ToolName::new("weather".to_owned()).expect("tool name");
    let call_id = ToolCallId::new("call-weather".to_owned()).expect("call ID");
    let arguments =
        CanonicalJson::parse(r#"{"city":"Paris"}"#, JsonBounds::value(limits)).expect("args");
    let replay_bytes = match dialect {
        WireDialect::GeminiInteractionsV1 => {
            br#"{"signature":"opaque-google-signature","type":"thought"}"#.to_vec()
        }
        _ => br#"{"thoughtSignature":"opaque-google-signature"}"#.to_vec(),
    };
    let replay = ReasoningReplay::new(
        Some(BoundedText::new("checked".to_owned(), limits).expect("summary")),
        replay_bytes,
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
    let messages = vec![
        Message::new(Role::User, vec![ContentBlock::Image(image)], limits).expect("user"),
        Message::new(
            Role::Assistant,
            vec![
                ContentBlock::Reasoning(replay),
                ContentBlock::ToolCall(
                    CompletedToolCall::new(call_id.clone(), name.clone(), arguments).expect("call"),
                ),
            ],
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
        .expect("tool"),
    ];
    let cache = if dialect == WireDialect::GeminiGenerateContentV1 {
        CachePolicy::Explicit(
            CacheKey::new("cachedContents/weather-context".to_owned()).expect("cache key"),
        )
    } else {
        CachePolicy::Disabled
    };
    peritus_model_protocol::ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("google-rich".to_owned()).expect("request ID"),
        messages,
        vec![ToolDefinition::new(
            name.clone(),
            Some(BoundedText::new("Read weather".to_owned(), limits).expect("description")),
            schema.clone(),
            true,
        )],
        ToolChoice::Specific(name),
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::JsonSchema {
                name: OutputName::new("weather_result".to_owned()).expect("output name"),
                schema,
                strict: true,
            },
            ReasoningPolicy::Effort { effort: ReasoningEffort::High, summary: SummaryPolicy::Auto },
            GenerationConfig::new(512, Vec::new(), None, None, None).expect("generation"),
            cache,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        limits,
    )
    .expect("request")
}
