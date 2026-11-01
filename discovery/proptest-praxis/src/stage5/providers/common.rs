//! Shared fixtures for actual provider adapters; expectations are original byte lists and JSON.

use peritus_model_protocol::*;
use peritus_provider_core::*;
use peritus_types::ProviderProfileId;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub type Metrics = BTreeMap<String, u64>;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Mode {
    OpenAi,
    Anthropic,
    GoogleGenerate,
    GoogleInteractions,
    CompatibleChat,
    CompatibleResponses,
    Codex,
    Claude,
    OpenCode,
}

impl Mode {
    pub fn runtime(self) -> bool {
        matches!(self, Self::Codex | Self::Claude | Self::OpenCode)
    }
    pub fn effort_supported(self, effort: u8) -> bool {
        effort == 0
            || match self {
                Self::CompatibleChat | Self::CompatibleResponses | Self::OpenCode => false,
                Self::Anthropic
                | Self::GoogleGenerate
                | Self::GoogleInteractions
                | Self::Claude => effort < 3,
                Self::OpenAi | Self::Codex => true,
            }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenAi => "openai_http",
            Self::Anthropic => "anthropic_http",
            Self::GoogleGenerate => "google_generate",
            Self::GoogleInteractions => "google_interactions",
            Self::CompatibleChat => "compatible_chat",
            Self::CompatibleResponses => "compatible_responses",
            Self::Codex => "codex_runtime",
            Self::Claude => "claude_runtime",
            Self::OpenCode => "opencode_runtime",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Fault {
    None,
    MissingTerminal,
    MalformedFrame,
    PartialTool,
    Duplicate,
    ConflictingId,
    ChangedResponse,
    Truncate(u8),
    ExtraTerminal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent provider fixture switches are serialized for replay"
)]
pub struct Case {
    pub mode: Mode,
    pub tool: bool,
    pub payload: String,
    pub model: u8,
    pub effort: u8,
    pub input: u16,
    pub output: u16,
    pub usage: bool,
    pub fault: Fault,
    pub chunks: Vec<u8>,
    pub cancel_after: Option<u8>,
    pub retry: bool,
    pub variant: bool,
    #[serde(default = "default_ids")]
    pub ids: bool,
    #[serde(default)]
    pub legacy: bool,
}

fn default_ids() -> bool {
    true
}

pub fn bump(m: &mut Metrics, name: &str) {
    *m.entry(name.into()).or_default() += 1;
}
pub fn strategy(modes: &[Mode], max_steps: usize) -> BoxedStrategy<Case> {
    let mode = prop::sample::select(modes.to_vec());
    let fault = prop_oneof![5=>Just(Fault::None),1=>Just(Fault::MissingTerminal),1=>Just(Fault::MalformedFrame),1=>Just(Fault::PartialTool),1=>Just(Fault::Duplicate),1=>Just(Fault::ConflictingId),1=>Just(Fault::ChangedResponse),1=>any::<u8>().prop_map(Fault::Truncate),1=>Just(Fault::ExtraTerminal)];
    (
        (
            mode,
            any::<bool>(),
            prop::collection::vec(
                prop::sample::select(vec!['a', 'Z', 'é', '中', '\n', '"', '\\', '🙂']),
                0..=16,
            ),
            0_u8..3,
            prop_oneof![5=>Just(0_u8),2=>1_u8..3,1=>Just(3_u8)],
            8_u16..64,
            4_u16..32,
        ),
        (
            any::<bool>(),
            fault,
            prop::collection::vec(1_u8..64, 0..=max_steps.min(24)),
            prop::option::weighted(0.15, 0_u8..5),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
        ),
    )
        .prop_map(
            |(
                (mode, tool, payload, model, effort, input, output),
                (usage, fault, chunks, cancel_after, retry, variant, ids, legacy),
            )| Case {
                mode,
                tool: tool || fault == Fault::PartialTool,
                payload: format!("payload:{}", payload.into_iter().collect::<String>()),
                model,
                effort,
                input,
                output,
                usage,
                fault,
                chunks,
                cancel_after,
                retry: retry && !matches!(mode, Mode::Codex | Mode::Claude),
                variant,
                ids: ids || matches!(fault, Fault::Duplicate | Fault::ConflictingId),
                legacy,
            },
        )
        .boxed()
}

/// Pair each admitted malformed transcript with the same clean request/response payload. This
/// prevents arbitrary generated setup or normal decoding failures from qualifying a negative case.
pub fn positive_control(case: &Case) -> Option<Case> {
    (case.fault != Fault::None
        && case.cancel_after.is_none()
        && case.mode.effort_supported(case.effort))
    .then(|| Case { fault: Fault::None, ..case.clone() })
}

pub fn example(mode: Mode) -> Case {
    Case {
        mode,
        tool: true,
        payload: "distinct-é-中🙂\n\"".into(),
        model: 1,
        effort: 0,
        input: 17,
        output: 9,
        usage: true,
        fault: Fault::None,
        chunks: vec![1, 2, 7, 3, 31],
        cancel_after: None,
        retry: false,
        variant: false,
        ids: true,
        legacy: false,
    }
}
pub fn examples(modes: &[Mode]) -> Vec<Case> {
    let mut values = Vec::new();
    for mode in modes {
        let base = example(*mode);
        values.push(base.clone());
        values.push(Case { ids: false, ..base.clone() });
        values.push(Case { tool: false, payload: "hello-é🙂".into(), ..base.clone() });
        values.push(Case { fault: Fault::PartialTool, ..base.clone() });
        values.push(Case { fault: Fault::MissingTerminal, ..base.clone() });
        values.push(Case { fault: Fault::Duplicate, ..base.clone() });
        values.push(Case { fault: Fault::ConflictingId, ..base.clone() });
        values.push(Case { fault: Fault::ChangedResponse, ..base.clone() });
        values.push(Case { effort: 1, ..base.clone() });
        values.push(Case { effort: 3, ..base.clone() });
        values.push(Case { cancel_after: Some(0), ..base.clone() });
        values.push(Case { cancel_after: Some(2), ..base.clone() });
        values.push(Case { usage: false, ..base.clone() });
        if !matches!(mode, Mode::Codex | Mode::Claude) {
            values.push(Case { retry: true, ..base.clone() });
        }
        values.push(Case { variant: true, ..base });
    }
    values
}

pub struct SyntheticCredential;
impl CredentialSource for SyntheticCredential {
    fn resolve(&self, reference: &CredentialReference) -> Result<Credential, ProviderCoreError> {
        if reference.as_str() != "praxis-offline" {
            return Err(ProviderCoreError::configuration(
                "fixture_credential",
                "unexpected reference",
            ));
        }
        Credential::new(b"praxis-synthetic-never-a-user-credential".to_vec())
    }
}
pub fn credential() -> Result<CredentialReference, String> {
    CredentialReference::new("praxis-offline".into()).map_err(|e| e.to_string())
}

pub fn model(case: &Case) -> String {
    if case.mode == Mode::OpenCode {
        format!(
            "{}/praxis-selected-{}",
            if case.variant { "opencode-go" } else { "opencode" },
            case.model
        )
    } else {
        format!("praxis-selected-{}", case.model)
    }
}

pub fn profile(mode: Mode) -> Result<ProviderProfile, String> {
    let (provider, dialect) = match mode {
        Mode::OpenAi => ("openai", WireDialect::OpenAiResponses),
        Mode::Anthropic => ("anthropic", WireDialect::AnthropicMessages),
        Mode::GoogleGenerate => ("google", WireDialect::GeminiGenerateContentV1),
        Mode::GoogleInteractions => ("google", WireDialect::GeminiInteractionsV1),
        Mode::CompatibleChat => ("praxis-compatible", WireDialect::CompatibleChatCompletions),
        Mode::CompatibleResponses => ("praxis-compatible", WireDialect::CompatibleResponses),
        Mode::Codex => ("openai", WireDialect::OpenAiCodexRuntime),
        Mode::Claude => ("anthropic", WireDialect::AnthropicClaudeRuntime),
        Mode::OpenCode => ("opencode", WireDialect::OpenCodeRuntime),
    };
    let mut capabilities =
        vec![Capability::ToolCalls, Capability::ParallelToolCalls, Capability::UsageDetail];
    if mode != Mode::Claude {
        capabilities.push(Capability::Streaming);
    }
    if !matches!(mode, Mode::CompatibleChat | Mode::CompatibleResponses | Mode::OpenCode) {
        capabilities.push(Capability::ReasoningControls);
    }
    ProviderProfile::new(
        ProviderProfileId::new([91; 16]).map_err(|e| format!("{e:?}"))?,
        1,
        ProviderName::new(provider.into()).map_err(|e| e.to_string())?,
        ModelName::new(
            if mode == Mode::OpenCode { "opencode/praxis-initial" } else { "praxis-initial" }
                .into(),
        )
        .map_err(|e| e.to_string())?,
        dialect,
        CapabilityMatrix::new(&capabilities, &[]).map_err(|e| e.to_string())?,
        CapabilityProvenance::Probed,
        ModelLimits::new(100_000, 4096, 64, 8, 1).map_err(|e| e.to_string())?,
        if mode.runtime() {
            OutputLimitEnforcement::Advisory
        } else {
            OutputLimitEnforcement::ProviderEnforced
        },
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .map_err(|e| e.to_string())
}

fn request_text(case: &Case) -> String {
    format!("Synthetic provider request {}. Preserve this exact text: {}", case.model, case.payload)
}

pub fn request(profile: &ProviderProfile, case: &Case) -> Result<ModelRequest, String> {
    let limits = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(
            &[Capability::ToolCalls],
            &[Capability::Streaming, Capability::UsageDetail, Capability::ReasoningControls],
            profile.limits(),
        )
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let tool=ToolDefinition::new(ToolName::new("lookup".into()).map_err(|e|e.to_string())?,None,JsonSchema::parse(r#"{"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false}"#,SchemaDialect::Draft202012,JsonBounds::schema(limits)).map_err(|e|e.to_string())?,false);
    let reasoning = if case.effort == 0 {
        ReasoningPolicy::Disabled
    } else {
        ReasoningPolicy::Effort {
            effort: match case.effort {
                1 => ReasoningEffort::Low,
                2 => ReasoningEffort::High,
                _ => ReasoningEffort::Ultra,
            },
            summary: SummaryPolicy::None,
        }
    };
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("praxis-provider-boundary".into()).map_err(|e| e.to_string())?,
        vec![
            Message::new(
                Role::User,
                vec![ContentBlock::Text(
                    BoundedText::new(request_text(case), limits).map_err(|e| e.to_string())?,
                )],
                limits,
            )
            .map_err(|e| e.to_string())?,
        ],
        vec![tool],
        if case.tool {
            ToolChoice::Specific(ToolName::new("lookup".into()).map_err(|e| e.to_string())?)
        } else {
            ToolChoice::None
        },
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::Text,
            reasoning,
            GenerationConfig::new(256, vec![], None, None, None).map_err(|e| e.to_string())?,
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            vec![],
        ),
        limits,
    )
    .map_err(|e| e.to_string())
}

pub fn retry_policy() -> Result<RetryPolicy, String> {
    RetryPolicy::new(
        2,
        [
            Duration::from_millis(1),
            Duration::from_millis(1),
            Duration::from_millis(1),
            Duration::from_secs(2),
        ],
        1024 * 1024,
    )
    .map_err(|e| e.to_string())
}

#[derive(Clone)]
struct Frame {
    event: Option<String>,
    id: String,
    value: Option<Value>,
    done: bool,
}

fn template(case: &Case) -> &'static str {
    match (case.mode, case.tool) {
        (Mode::OpenAi, false) => include_str!(
            "../../../../../crates/model/peritus-provider-openai/fixtures/v1/success.sse"
        ),
        (Mode::OpenAi, true) => include_str!(
            "../../../../../crates/model/peritus-provider-openai/fixtures/v1/tool-reasoning.sse"
        ),
        (Mode::Anthropic, false) => include_str!(
            "../../../../../crates/model/peritus-provider-anthropic/fixtures/v1/text.sse"
        ),
        (Mode::Anthropic, true) => include_str!(
            "../../../../../crates/model/peritus-provider-anthropic/fixtures/v1/tool_thinking.sse"
        ),
        (Mode::GoogleGenerate, false) => include_str!(
            "../../../../../crates/model/peritus-provider-google/fixtures/v1/generate_success.sse"
        ),
        (Mode::GoogleGenerate, true) => include_str!(
            "../../../../../crates/model/peritus-provider-google/fixtures/v1/generate_tool_thinking.sse"
        ),
        (Mode::GoogleInteractions, false) => include_str!(
            "../../../../../crates/model/peritus-provider-google/fixtures/v1/interactions_success.sse"
        ),
        (Mode::GoogleInteractions, true) => include_str!(
            "../../../../../crates/model/peritus-provider-google/fixtures/v1/interactions_tool_thinking.sse"
        ),
        (Mode::CompatibleChat, false) => include_str!(
            "../../../../../crates/model/peritus-provider-compatible/fixtures/v1/chat-success.sse"
        ),
        (Mode::CompatibleChat, true) => include_str!(
            "../../../../../crates/model/peritus-provider-compatible/fixtures/v1/chat-tool.sse"
        ),
        (Mode::CompatibleResponses, false) => include_str!(
            "../../../../../crates/model/peritus-provider-compatible/fixtures/v1/responses-success.sse"
        ),
        (Mode::CompatibleResponses, true) => include_str!(
            "../../../../../crates/model/peritus-provider-compatible/fixtures/v1/responses-tool.sse"
        ),
        _ => "",
    }
}

fn usage(case: &Case) -> Value {
    let total = u64::from(case.input) + u64::from(case.output);
    match case.mode {
        Mode::Anthropic => {
            json!({"input_tokens":case.input,"output_tokens":case.output,"cache_read_input_tokens":2,"cache_creation_input_tokens":1})
        }
        Mode::GoogleGenerate => {
            json!({"promptTokenCount":case.input,"cachedContentTokenCount":2,"candidatesTokenCount":case.output,"thoughtsTokenCount":2,"toolUsePromptTokenCount":1,"totalTokenCount":total})
        }
        Mode::GoogleInteractions => {
            json!({"total_input_tokens":case.input,"total_cached_tokens":2,"total_output_tokens":case.output,"total_thought_tokens":2,"total_tool_use_tokens":1,"total_tokens":total})
        }
        Mode::CompatibleChat => {
            json!({"prompt_tokens":case.input,"completion_tokens":case.output,"total_tokens":total})
        }
        Mode::CompatibleResponses => {
            json!({"input_tokens":case.input,"output_tokens":case.output,"total_tokens":total})
        }
        _ => {
            json!({"input_tokens":case.input,"output_tokens":case.output,"input_tokens_details":{"cached_tokens":2},"output_tokens_details":{"reasoning_tokens":2},"total_tokens":total})
        }
    }
}

fn rewrite(value: &mut Value, case: &Case, args: &str, delta_index: &mut usize) {
    if let Value::Object(object) = value {
        for (key, field) in object.iter_mut() {
            if matches!(key.as_str(), "model" | "modelVersion") {
                *field = Value::String(model(case));
            } else if key == "name"
                && field.as_str().is_some_and(|v| matches!(v, "lookup" | "weather"))
            {
                *field = json!("lookup");
            } else if key == "usage" || key == "usageMetadata" {
                *field = if case.usage { usage(case) } else { json!({}) };
            } else if key == "args" {
                *field = json!({"value":case.payload});
            } else if matches!(key.as_str(), "arguments" | "partial_json") && field.is_string() {
                if key == "arguments"
                    && field.as_str().is_some_and(|v| v.starts_with('{') && v.ends_with('}'))
                {
                    *field = json!(args);
                } else if !field.as_str().unwrap_or_default().is_empty() {
                    let split = args
                        .char_indices()
                        .nth(args.chars().count() / 2)
                        .map_or(args.len(), |(i, _)| i);
                    *field = json!(if (*delta_index).is_multiple_of(2) {
                        &args[..split]
                    } else {
                        &args[split..]
                    });
                    *delta_index += 1;
                }
            } else {
                rewrite(field, case, args, delta_index);
            }
        }
        // Responses and Anthropic tool fragments live under a generic delta string.
        let kind = object.get("type").and_then(Value::as_str).unwrap_or_default().to_owned();
        if kind == "response.function_call_arguments.delta" {
            let split =
                args.char_indices().nth(args.chars().count() / 2).map_or(args.len(), |(i, _)| i);
            object.insert(
                "delta".into(),
                json!(if (*delta_index).is_multiple_of(2) {
                    &args[..split]
                } else {
                    &args[split..]
                }),
            );
            *delta_index += 1;
        }
        if kind == "response.function_call_arguments.done" {
            object.insert("arguments".into(), json!(args));
        }
        if let Some(field) = object.get_mut("text")
            && field.as_str().is_some_and(|v| matches!(v, "hello" | "héllo" | "hel" | "lo"))
        {
            *field = json!(case.payload);
        }
        if let Some(field) = object.get_mut("content")
            && field.as_str().is_some_and(|v| matches!(v, "hel" | "lo"))
        {
            *field = json!(case.payload);
        }
        if matches!(kind.as_str(), "response.output_text.delta" | "response.output_text.done") {
            let key = if kind.ends_with("delta") { "delta" } else { "text" };
            object.insert(key.into(), json!(case.payload));
        }
    } else if let Value::Array(values) = value {
        for value in values {
            rewrite(value, case, args, delta_index);
        }
    }
}

#[allow(clippy::too_many_lines, reason = "fixture wire forms stay together for source review")]
pub fn wire(case: &Case) -> Result<Vec<u8>, String> {
    let args = serde_json::to_string(&json!({"value":case.payload})).map_err(|e| e.to_string())?;
    let mut frames = Vec::new();
    let mut delta = 0;
    for block in template(case).split("\n\n") {
        let event = block.lines().find_map(|line| line.strip_prefix("event: ")).map(str::to_owned);
        let Some(data) = block.lines().find_map(|line| line.strip_prefix("data: ")) else {
            continue;
        };
        if data == "[DONE]" {
            frames.push(Frame { event: None, id: String::new(), value: None, done: true });
            continue;
        }
        let mut value: Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
        rewrite(&mut value, case, &args, &mut delta);
        // Text Chat fixtures use two deltas; keep only the first content to preserve one payload.
        if case.mode == Mode::CompatibleChat
            && !case.tool
            && value.pointer("/choices/0/delta/content").is_some()
            && frames.iter().any(|frame: &Frame| {
                frame
                    .value
                    .as_ref()
                    .is_some_and(|v| v.pointer("/choices/0/delta/content").is_some())
            })
        {
            continue;
        }
        frames.push(Frame {
            event,
            id: format!("praxis-{}", frames.len() + 1),
            value: Some(value),
            done: false,
        });
    }
    if case.mode == Mode::CompatibleChat && case.tool && case.usage {
        let v = json!({"id":"chat_tool","object":"chat.completion.chunk","model":model(case),"choices":[],"usage":usage(case)});
        let index = frames.len().saturating_sub(1);
        frames.insert(
            index,
            Frame { event: None, id: "praxis-usage".into(), value: Some(v), done: false },
        );
    }
    let terminal = |frame: &Frame| {
        frame.done
            || frame.value.as_ref().is_some_and(|v| {
                matches!(
                    v.get("type").or_else(|| v.get("event_type")).and_then(Value::as_str),
                    Some("response.completed" | "message_stop" | "interaction.completed")
                ) || v.pointer("/candidates/0/finishReason").is_some()
            })
    };
    match case.fault {
        Fault::MissingTerminal => frames.retain(|frame| !terminal(frame)),
        Fault::PartialTool => {
            if case.mode == Mode::GoogleGenerate {
                frames.clear();
            } else {
                let partial = frames.iter().position(|frame| {
                    frame.value.as_ref().is_some_and(|v| {
                        v.get("type").and_then(Value::as_str)
                            == Some("response.function_call_arguments.delta")
                            || v.pointer("/delta/type").and_then(Value::as_str).is_some_and(
                                |kind| matches!(kind, "input_json_delta" | "arguments_delta"),
                            )
                            || v.pointer("/choices/0/delta/tool_calls/0/function/arguments")
                                .is_some()
                    })
                });
                // End transport after the first actual tool JSON fragment, before any finalizer.
                if let Some(index) = partial {
                    frames.truncate(index + 1);
                } else {
                    frames.truncate(1);
                }
            }
        }
        Fault::Duplicate | Fault::ConflictingId => {
            if let Some(index) = frames
                .iter()
                .position(|frame| {
                    frame.value.as_ref().is_some_and(|v| {
                        matches!(
                            v.get("type").and_then(Value::as_str),
                            Some(
                                "response.function_call_arguments.delta"
                                    | "response.output_text.delta"
                            )
                        ) || v.pointer("/delta/type").and_then(Value::as_str).is_some_and(|kind| {
                            matches!(
                                kind,
                                "input_json_delta" | "arguments_delta" | "text_delta" | "text"
                            )
                        }) || v
                            .pointer("/choices/0/delta/tool_calls/0/function/arguments")
                            .is_some()
                            || v.pointer("/choices/0/delta/content").is_some()
                            || v.get("candidates").is_some()
                    })
                })
                .or_else(|| (!frames.is_empty()).then_some(0))
            {
                let mut duplicate = frames[index].clone();
                if case.fault == Fault::ConflictingId
                    && let Some(v) = duplicate.value.as_mut()
                {
                    corrupt_semantic(v);
                }
                frames.insert(index + 1, duplicate);
            }
        }
        Fault::ChangedResponse => {
            if let Some(frame) =
                frames.iter_mut().rev().find(|frame| frame.value.is_some() && terminal(frame))
            {
                let value = frame.value.as_mut().ok_or("missing terminal value")?;
                if let Some(object) = value.get_mut("response") {
                    object["id"] = json!("foreign-response");
                } else if let Some(object) = value.get_mut("interaction") {
                    object["id"] = json!("foreign-response");
                } else if value.get("responseId").is_some() {
                    value["responseId"] = json!("foreign-response");
                }
            }
            if case.mode == Mode::Anthropic
                && let Some(mut first) = frames.first().cloned()
            {
                first.id = "praxis-foreign-response".into();
                first.value.as_mut().ok_or("missing start")?["message"]["id"] =
                    json!("foreign-response");
                frames.insert(1, first);
            }
            if case.mode == Mode::CompatibleChat
                && let Some(frame) = frames.iter_mut().find(|f| {
                    f.value.as_ref().is_some_and(|v| {
                        v.pointer("/choices/0/finish_reason").is_some_and(|v| !v.is_null())
                    })
                })
            {
                frame.value.as_mut().ok_or("missing chat frame")?["id"] = json!("foreign-response");
            }
        }
        Fault::ExtraTerminal => {
            if let Some(frame) = frames.iter().find(|frame| terminal(frame) && !frame.done).cloned()
            {
                frames.push(frame);
            }
        }
        _ => {}
    }
    let mut bytes = Vec::new();
    for frame in frames {
        if frame.done {
            bytes.extend_from_slice(b"data: [DONE]\n\n");
            continue;
        }
        if let Some(event) = frame.event {
            bytes.extend_from_slice(format!("event: {event}\n").as_bytes());
        }
        if case.ids {
            bytes.extend_from_slice(format!("id: {}\n", frame.id).as_bytes());
        }
        bytes.extend_from_slice(
            format!(
                "data: {}\n\n",
                serde_json::to_string(&frame.value.ok_or("missing value")?)
                    .map_err(|e| e.to_string())?
            )
            .as_bytes(),
        );
    }
    if case.fault == Fault::MalformedFrame {
        bytes = b"event: invalid\ndata: {broken\n\n".to_vec();
    }
    if let Fault::Truncate(at) = case.fault {
        bytes.truncate(bytes.len() * usize::from(at) / 256);
    }
    Ok(bytes)
}

fn corrupt_semantic(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if matches!(
                    key.as_str(),
                    "text" | "content" | "delta" | "partial_json" | "arguments"
                ) && value.is_string()
                {
                    *value = json!(format!("{}-conflicting", value.as_str().unwrap_or_default()));
                } else if key == "args" {
                    *value = json!({"value":"foreign-scripted-argument"});
                } else {
                    corrupt_semantic(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                corrupt_semantic(value);
            }
        }
        _ => {}
    }
}

pub struct Server {
    pub endpoint: Endpoint,
    pub requests: Arc<Mutex<Vec<(String, Value)>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<Result<(), String>>>,
}

impl Server {
    pub fn new(case: &Case) -> Result<Self, String> {
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let endpoint =
            Endpoint::new(format!("http://{}", listener.local_addr().map_err(|e| e.to_string())?))
                .map_err(|e| e.to_string())?;
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let body = wire(case)?;
        let chunks = case.chunks.clone();
        let retry = case.retry;
        let worker = thread::spawn(move || {
            let started = Instant::now();
            let mut attempt = 0;
            while !stopped.load(Ordering::Acquire) && started.elapsed() < Duration::from_secs(8) {
                let (mut socket, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(e) => return Err(e.to_string()),
                };
                socket.set_read_timeout(Some(Duration::from_secs(2))).map_err(|e| e.to_string())?;
                socket
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .map_err(|e| e.to_string())?;
                let request = read_http(&mut socket)?;
                seen.lock().map_err(|e| e.to_string())?.push(request);
                attempt += 1;
                if retry && attempt == 1 {
                    let rejection =
                        b"{\"error\":{\"type\":\"server_error\",\"message\":\"fixture rejected\"}}";
                    let headers = format!(
                        "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        rejection.len()
                    );
                    socket
                        .write_all(headers.as_bytes())
                        .and_then(|()| socket.write_all(rejection))
                        .ok();
                    continue;
                }
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").ok();
                let mut offset = 0;
                let mut ordinal = 0;
                while offset < body.len() && !stopped.load(Ordering::Acquire) {
                    let size = usize::from(
                        chunks.get(ordinal % chunks.len().max(1)).copied().unwrap_or(251),
                    )
                    .max(1);
                    let end = (offset + size).min(body.len());
                    let header = format!("{:x}\r\n", end - offset);
                    if socket
                        .write_all(header.as_bytes())
                        .and_then(|()| socket.write_all(&body[offset..end]))
                        .and_then(|()| socket.write_all(b"\r\n"))
                        .is_err()
                    {
                        break;
                    }
                    offset = end;
                    ordinal += 1;
                }
                socket.write_all(b"0\r\n\r\n").ok();
                break;
            }
            Ok(())
        });
        Ok(Self { endpoint, requests, stop, worker: Some(worker) })
    }
    pub fn finish(mut self) -> Result<Vec<(String, Value)>, String> {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| "loopback worker panicked")??;
        }
        let values = self.requests.lock().map_err(|e| e.to_string())?.clone();
        Ok(values)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn read_http(socket: &mut TcpStream) -> Result<(String, Value), String> {
    let mut bytes = Vec::new();
    let end = loop {
        let mut byte = [0];
        socket.read_exact(&mut byte).map_err(|e| e.to_string())?;
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            break bytes.len();
        }
        if bytes.len() > 64 * 1024 {
            return Err("loopback headers too large".into());
        }
    };
    let headers = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let route = headers.lines().next().ok_or("missing request line")?.to_owned();
    let size = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(str::trim)
                .map(str::to_owned)
        })
        .ok_or("missing content length")?
        .parse::<usize>()
        .map_err(|e| e.to_string())?;
    if size > 1024 * 1024 {
        return Err("loopback request too large".into());
    }
    bytes.resize(end + size, 0);
    socket.read_exact(&mut bytes[end..]).map_err(|e| e.to_string())?;
    Ok((route, serde_json::from_slice(&bytes[end..]).map_err(|e| e.to_string())?))
}

pub async fn drive(
    provider: Arc<dyn ModelProvider>,
    case: &Case,
    metrics: &mut Metrics,
) -> Result<(), String> {
    drive_owned(provider, case, metrics, None).await
}

#[allow(clippy::too_many_lines, reason = "one owned provider turn and exact reducer oracle")]
pub async fn drive_owned(
    provider: Arc<dyn ModelProvider>,
    case: &Case,
    metrics: &mut Metrics,
    ready: Option<std::path::PathBuf>,
) -> Result<(), String> {
    let initial_model = provider.profile().model().as_str().to_owned();
    let initial = Arc::clone(&provider);
    let provider = provider
        .select_model(ModelName::new(model(case)).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if provider.profile().model().as_str() != model(case) {
        return Err("explicit model selection was not retained by provider".into());
    }
    if initial.profile().model().as_str() != initial_model {
        return Err("selecting a new provider changed the original provider profile".into());
    }
    bump(metrics, "immutable_model_selections");
    let request = match request(provider.profile(), case) {
        Ok(request) => request,
        Err(_error) if !case.mode.effort_supported(case.effort) => {
            bump(metrics, "unsupported_effort_request_rejections");
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    let cancellation = CancellationToken::new();
    if case.cancel_after == Some(0) {
        let _ = cancellation.cancel();
    }
    let cancel_worker =
        if case.cancel_after.is_some_and(|n| n > 0) && case.mode.effort_supported(case.effort) {
            ready.map(|ready| {
                let cancellation = cancellation.clone();
                tokio::spawn(async move {
                    let start = Instant::now();
                    while !ready.exists() && start.elapsed() < Duration::from_secs(2) {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                    let reached = ready.exists();
                    let _ = cancellation.cancel();
                    reached
                })
            })
        } else {
            None
        };
    let started = provider.start(request, cancellation.clone()).await;
    // Codex starts its owned process when pulled; awaiting the ready task here would
    // cancel a still-unpolled operation instead of the admitted native process.
    let mut stream = match started {
        Ok(stream) => stream,
        Err(error)
            if case.cancel_after.is_some() && error.kind() == ProviderCoreErrorKind::Cancelled =>
        {
            if let Some(worker) = cancel_worker {
                if !worker.await.map_err(|e| e.to_string())? {
                    return Err("owned cancellation never reached its ready marker".into());
                }
                bump(metrics, "owned_runtime_ready_cancellations");
            } else {
                bump(metrics, "cancelled_before_submission");
            }
            return Ok(());
        }
        Err(_error) if !case.mode.effort_supported(case.effort) => {
            bump(metrics, "unsupported_effort_adapter_rejections");
            return Ok(());
        }
        Err(error)
            if case.fault != Fault::None
                && error.kind() == ProviderCoreErrorKind::MalformedStream =>
        {
            bump(metrics, "early_malformed_rejections");
            return Ok(());
        }
        Err(error) => return Err(error.to_string()),
    };
    if !case.mode.effort_supported(case.effort) {
        return Err("unsupported selected effort silently accepted".into());
    }
    let mut items: BTreeMap<String, (String, Vec<u8>, bool)> = BTreeMap::new();
    let mut calls = BTreeMap::new();
    let mut completed = BTreeSet::new();
    let mut text = Vec::new();
    let mut terminal = 0;
    let mut success = false;
    let mut usage = None;
    let mut sequence = 0;
    let mut pulls = 0;
    let mut reducer =
        ResponseReducer::new(provider.profile().provider().clone(), ProtocolLimits::PRODUCTION);
    let mut seen = BTreeMap::new();
    let mut trace = Vec::new();
    let mut boundary_rejected = false;
    while let Some(envelope) = stream.pull().await.map_err(|e| e.to_string())? {
        pulls += 1;
        if envelope.sequence() != sequence + 1 {
            return Err("normalized local sequence skipped or repeated".into());
        }
        sequence = envelope.sequence();
        bump(metrics, "normalized_events");
        let mut duplicate = false;
        let mut conflict = false;
        if let Some(id) = envelope.provider_event_id() {
            let key = id.expose_for_wire().to_owned();
            let identity = (envelope.provider_digest(), envelope.provider_sequence());
            if let Some(old) = seen.get(&key) {
                duplicate = *old == identity;
                conflict = !duplicate;
            } else {
                seen.insert(key, identity);
            }
        }
        let transition = reducer.push(envelope.clone());
        trace.push(json!({"sequence":envelope.sequence(),"provider_sequence":envelope.provider_sequence(),"provider_event_id":envelope.provider_event_id().map(EventId::expose_for_wire),"raw_digest":envelope.provider_digest().as_bytes(),"event":event_json(envelope.event()),"downstream":format!("{transition:?}"),"completed_items":reducer.completed_items().len()}));
        match transition {
            Err(error) => {
                if case.fault != Fault::None && case.fault != Fault::Duplicate {
                    boundary_rejected = true;
                    bump(metrics, "downstream_boundary_rejections");
                    break;
                }
                return Err(format!(
                    "actual downstream rejected an ordinary/duplicate scripted transcript: {error}; trace={}",
                    serde_json::to_string(&trace).map_err(|e| e.to_string())?
                ));
            }
            Ok(ReducerTransition::DuplicateIgnored) if duplicate => {
                bump(metrics, "downstream_exact_duplicates");
                continue;
            }
            Ok(_) if duplicate || conflict => {
                return Err(
                    "actual downstream disagrees with independent raw identity table".into()
                );
            }
            Ok(_) => {}
        }
        match envelope.event() {
            ModelEvent::ItemStarted { item_id, .. } => {
                let id = item_id.expose_for_wire().to_owned();
                if items.insert(id, (String::new(), Vec::new(), false)).is_some() {
                    return Err("output item identity reused".into());
                }
            }
            ModelEvent::ToolCallStarted { item_id, call_id, name } => {
                if name.as_str() != "lookup" {
                    return Err("adapter changed declared tool name".into());
                }
                let id = item_id.expose_for_wire().to_owned();
                let call = call_id.expose_for_wire().to_owned();
                let item = items.get_mut(&id).ok_or("call preceded item")?;
                item.0 = call.clone();
                if calls.insert(call, id).is_some() {
                    return Err("tool call identity reused".into());
                }
            }
            ModelEvent::ToolArgumentDelta { call_id, fragment } => {
                let id = calls.get(call_id.expose_for_wire()).ok_or("arguments preceded call")?;
                let item = items.get_mut(id).ok_or("call item missing")?;
                if item.2 {
                    return Err("tool arguments arrived after item completion".into());
                }
                item.1.extend_from_slice(fragment.expose());
            }
            ModelEvent::TextDelta { fragment, .. } => text.extend_from_slice(fragment.expose()),
            ModelEvent::ItemCompleted(id) => {
                let id = id.expose_for_wire().to_owned();
                let item = items.get_mut(&id).ok_or("completion preceded item")?;
                if item.2 {
                    return Err("item completed twice".into());
                }
                item.2 = true;
                if !item.0.is_empty() {
                    let args: Value = serde_json::from_slice(&item.1)
                        .map_err(|_| "partial/malformed tool JSON became a completed call")?;
                    if args != json!({"value":case.payload}) {
                        return Err(format!(
                            "tool args changed or partial args dispatched: {args}"
                        ));
                    }
                    completed.insert(item.0.clone());
                    bump(metrics, "complete_inert_tool_calls");
                }
            }
            ModelEvent::Usage(value) => usage = Some(value.counters()),
            ModelEvent::ResponseCompleted => {
                terminal += 1;
                success = true;
            }
            ModelEvent::ResponseFailed(_) => {
                terminal += 1;
                bump(metrics, "normalized_failures");
            }
            ModelEvent::ResponseCancelled => {
                terminal += 1;
                bump(metrics, "normalized_cancellations");
            }
            _ => {}
        }
        if case.cancel_after.is_some_and(|n| usize::from(n) == pulls) && terminal == 0 {
            let _ = cancellation.cancel();
        }
        if pulls > 2048 {
            return Err("normalized stream exceeded fixture bound".into());
        }
    }
    if let Some(worker) = cancel_worker {
        if !worker.await.map_err(|e| e.to_string())? {
            return Err("owned cancellation never reached its ready marker".into());
        }
        bump(metrics, "owned_runtime_ready_cancellations");
    }
    if boundary_rejected {
        let _ = cancellation.cancel();
        bump(metrics, case.mode.label());
        return Ok(());
    }
    if terminal != 1 || !stream.terminal_observed() {
        return Err(format!(
            "stream did not deliver exactly one terminal through the actual downstream consumer; trace={}",
            serde_json::to_string(&trace).map_err(|e| e.to_string())?
        ));
    }
    if stream.pull().await.map_err(|e| e.to_string())?.is_some() {
        return Err("terminal stream resumed".into());
    }
    if success {
        if !matches!(
            reducer.terminal(),
            Some(TerminalOutcome::Succeeded { .. } | TerminalOutcome::RequiresAction { .. })
        ) {
            return Err(format!(
                "adapter completed but downstream has no usable terminal; trace={}",
                serde_json::to_string(&trace).map_err(|e| e.to_string())?
            ));
        }
        let expected_text =
            if case.tool && case.mode != Mode::Claude { "" } else { case.payload.as_str() };
        if text != expected_text.as_bytes() {
            return Err(format!("normalized scripted public text changed: {text:?}"));
        }
        if case.tool {
            if completed.len() != 1 {
                return Err("successful tool turn lacks exactly one complete inert call".into());
            }
        } else if text != case.payload.as_bytes() {
            return Err(format!(
                "normalized text changed: expected {:?}, got {:?}",
                case.payload, text
            ));
        }
        let actual_text = reducer
            .completed_items()
            .iter()
            .filter_map(|item| {
                if let ReducedItem::Text { text, .. } = item {
                    Some(text.expose_for_wire())
                } else {
                    None
                }
            })
            .collect::<String>();
        let actual_calls = reducer
            .completed_items()
            .iter()
            .filter_map(|item| {
                if let ReducedItem::ToolCall { call, .. } = item { Some(call) } else { None }
            })
            .collect::<Vec<_>>();
        if case.tool
            && (actual_calls.len() != 1
                || actual_calls[0].name().as_str() != "lookup"
                || serde_json::from_slice::<Value>(actual_calls[0].arguments().canonical_bytes())
                    .map_err(|e| e.to_string())?
                    != json!({"value":case.payload}))
        {
            return Err(format!(
                "downstream lost/changed complete tool result; trace={}",
                serde_json::to_string(&trace).map_err(|e| e.to_string())?
            ));
        }
        if actual_text != expected_text {
            return Err(format!(
                "downstream text differs from original scripted content; trace={}",
                serde_json::to_string(&trace).map_err(|e| e.to_string())?
            ));
        }
        if case.usage {
            let actual = usage.ok_or("reported usage disappeared")?;
            let factor = if case.mode == Mode::OpenCode && case.retry { 2 } else { 1 };
            if actual.input_tokens() != Some(u64::from(case.input) * factor)
                || actual.output_tokens() != Some(u64::from(case.output) * factor)
                || actual.total_tokens()
                    != if case.mode == Mode::Claude {
                        None
                    } else {
                        Some((u64::from(case.input) + u64::from(case.output)) * factor)
                    }
            {
                return Err(format!("normalized usage changed: {actual:?}"));
            }
            bump(metrics, "verified_usage");
            let expected_cache = match case.mode {
                Mode::CompatibleChat | Mode::CompatibleResponses => None,
                _ => Some(2 * factor),
            };
            let expected_creation =
                if matches!(case.mode, Mode::Anthropic | Mode::Claude | Mode::OpenCode) {
                    Some(factor)
                } else {
                    None
                };
            let expected_reasoning = if matches!(
                case.mode,
                Mode::OpenAi | Mode::GoogleGenerate | Mode::GoogleInteractions | Mode::OpenCode
            ) {
                Some(2 * factor)
            } else {
                None
            };
            let expected_tools =
                if matches!(case.mode, Mode::GoogleGenerate | Mode::GoogleInteractions) {
                    Some(1)
                } else {
                    None
                };
            if actual.cached_input_tokens() != expected_cache
                || actual.cache_creation_input_tokens() != expected_creation
                || actual.reasoning_output_tokens() != expected_reasoning
                || actual.tool_tokens() != expected_tools
                || actual.provider_cost_microunits().is_some()
            {
                return Err(format!("normalized detailed usage changed: {actual:?}"));
            }
            if reducer.usage_high_water() != actual {
                return Err(format!(
                    "downstream usage differs from independently observed final counts: {actual:?} vs {:?}",
                    reducer.usage_high_water()
                ));
            }
        } else if usage.is_some_and(|v| {
            v.input_tokens().is_some() || v.output_tokens().is_some() || v.total_tokens().is_some()
        }) {
            return Err("missing usage was invented as a known counter".into());
        }
        bump(metrics, "successful_completions");
    } else if (case.fault == Fault::None || case.fault == Fault::Duplicate && !case.mode.runtime())
        && case.cancel_after.is_none()
    {
        return Err(format!(
            "ordinary supported scripted turn failed; trace={}",
            serde_json::to_string(&trace).map_err(|e| e.to_string())?
        ));
    }
    if matches!(
        case.fault,
        Fault::MissingTerminal
            | Fault::MalformedFrame
            | Fault::PartialTool
            | Fault::ChangedResponse
            | Fault::ConflictingId
    ) && success
    {
        return Err(format!("invalid transport/lifecycle accepted: {:?}", case.fault));
    }
    bump(metrics, case.mode.label());
    Ok(())
}

fn event_json(event: &ModelEvent) -> Value {
    match event {
        ModelEvent::ToolArgumentDelta { call_id, fragment } => {
            json!({"kind":"ToolArgumentDelta","call_id":call_id.expose_for_wire(),"bytes":fragment.expose()})
        }
        ModelEvent::TextDelta { item_id, fragment } => {
            json!({"kind":"TextDelta","item_id":item_id.expose_for_wire(),"bytes":fragment.expose()})
        }
        ModelEvent::ToolCallStarted { item_id, call_id, name } => {
            json!({"kind":"ToolCallStarted","item_id":item_id.expose_for_wire(),"call_id":call_id.expose_for_wire(),"name":name.as_str()})
        }
        ModelEvent::ItemCompleted(item) => {
            json!({"kind":"ItemCompleted","item_id":item.expose_for_wire()})
        }
        other => json!({"kind":format!("{other:?}")}),
    }
}

pub fn inspect_http(
    case: &Case,
    requests: &[(String, Value)],
    metrics: &mut Metrics,
) -> Result<(), String> {
    if case.cancel_after == Some(0) || !case.mode.effort_supported(case.effort) {
        if !requests.is_empty() {
            return Err("cancelled/unsupported request was submitted".into());
        }
        return Ok(());
    }
    if requests.len() != 1 + usize::from(case.retry) {
        return Err(format!("unexpected HTTP submission count: {}", requests.len()));
    }
    for (route, value) in requests {
        let selected = model(case);
        let model_matches = if case.mode == Mode::GoogleGenerate {
            route.split_whitespace().nth(1)
                == Some(format!("/v1/models/{selected}:streamGenerateContent?alt=sse").as_str())
        } else {
            value.get("model").and_then(Value::as_str) == Some(selected.as_str())
        };
        if !model_matches {
            return Err(format!(
                "selected model missing from actual HTTP request: {route} {value}"
            ));
        }
        let (role_path, text_path, role) = match case.mode {
            Mode::GoogleGenerate => ("/contents/0/role", "/contents/0/parts/0/text", "user"),
            Mode::GoogleInteractions => ("/input/0/type", "/input/0/content/0/text", "user_input"),
            Mode::Anthropic | Mode::CompatibleChat => {
                ("/messages/0/role", "/messages/0/content/0/text", "user")
            }
            Mode::OpenAi | Mode::CompatibleResponses => {
                ("/input/0/role", "/input/0/content/0/text", "user")
            }
            _ => return Err("non-HTTP provider passed to HTTP request oracle".into()),
        };
        if value.pointer(role_path).and_then(Value::as_str) != Some(role)
            || value.pointer(text_path).and_then(Value::as_str) != Some(request_text(case).as_str())
        {
            return Err(format!(
                "outbound request changed user role or exact Unicode/escaped text: {value}"
            ));
        }
        bump(metrics, "preserved_request_role_and_text");
        if case.effort != 0 {
            let effort = if case.effort == 1 {
                "low"
            } else if case.effort == 2 {
                "high"
            } else {
                "ultra"
            };
            let found = match case.mode {
                Mode::OpenAi => value.pointer("/reasoning/effort"),
                Mode::Anthropic => value.pointer("/output_config/effort"),
                Mode::GoogleGenerate => {
                    value.pointer("/generationConfig/thinkingConfig/thinkingLevel")
                }
                Mode::GoogleInteractions => value.pointer("/generation_config/thinking_level"),
                _ => None,
            };
            if found.and_then(Value::as_str) != Some(effort) {
                return Err(format!("selected effort missing from actual HTTP request: {value}"));
            }
            bump(metrics, "preserved_selected_effort");
        }
        bump(metrics, "preserved_selected_model");
    }
    if requests.len() == 2 {
        if requests[0] != requests[1] {
            return Err("retry changed exact request route/body".into());
        }
        bump(metrics, "conservative_http_retries");
    }
    Ok(())
}
