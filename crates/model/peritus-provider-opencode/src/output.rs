//! Fail-closed JSONL event and inert tool-envelope normalization.

use super::recovery::TurnRejection;
use peritus_model_protocol::{
    CanonicalJson, FailureCategory, JsonBounds, ModelEvent, ModelRequest, ProtocolLimits,
    ToolChoice, UsageCounters,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

pub(super) struct RuntimeTurn {
    pub content: String,
    pub tool_calls: Vec<RuntimeToolCall>,
    pub usage: UsageCounters,
    pub repairs: Vec<ModelEvent>,
}

pub(super) struct RuntimeToolCall {
    pub name: String,
    pub arguments: Map<String, Value>,
}

pub(super) struct NativeFailure {
    pub category: FailureCategory,
    pub status: Option<u16>,
    pub code: &'static str,
}

pub(super) enum Decoded {
    Turn(RuntimeTurn),
    Failure(NativeFailure),
    Rejected {
        reason: TurnRejection,
        usage: UsageCounters,
        /// Complete model-authored reply text that failed the private envelope contract.
        rejected: String,
    },
}

pub(super) fn decode(
    bytes: &[u8],
    request: &ModelRequest,
    runtime: &super::request::RuntimeRequest,
) -> Result<Decoded, ProviderCoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| super::malformed())?;
    let mut session = None;
    let mut message = None;
    let mut started = false;
    let mut finished = false;
    let mut result = String::new();
    let mut usage = UsageCounters::new(None, None, None, None, None, None, None, None);
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        CanonicalJson::parse(line, JsonBounds::value(ProtocolLimits::PRODUCTION))
            .map_err(|_| super::malformed())?;
        let event: Value = serde_json::from_str(line).map_err(|_| super::malformed())?;
        identity(&event, "sessionID", &mut session)?;
        let kind = event.get("type").and_then(Value::as_str).ok_or_else(super::malformed)?;
        if kind == "error" {
            return Ok(Decoded::Failure(classify(&event)));
        }
        if finished {
            return Err(super::malformed());
        }
        let part = event.get("part").ok_or_else(super::malformed)?;
        if part.get("sessionID").and_then(Value::as_str) != session.as_deref() {
            return Err(super::malformed());
        }
        identity(part, "messageID", &mut message)?;
        match kind {
            "step_start" if !started => {
                started = true;
            }
            "text" if started => {
                // GPT models can emit a separate commentary part before their final wire
                // object. Preserve the vendor's phase boundary; concatenating that prose
                // with final_answer corrupts otherwise valid JSON. Never strip unlabelled
                // prose or infer a missing final result.
                if part.pointer("/metadata/openai/phase").and_then(Value::as_str)
                    == Some("commentary")
                {
                    continue;
                }
                result.push_str(
                    part.get("text").and_then(Value::as_str).ok_or_else(super::malformed)?,
                );
            }
            "step_finish" if started => {
                if part.get("reason").and_then(Value::as_str) != Some("stop") {
                    return Err(super::malformed());
                }
                usage = counters(part)?;
                finished = true;
            }
            // Native operations are forbidden even if the CLI reports that one failed.
            _ => return Err(super::malformed()),
        }
    }
    if !finished {
        return Err(super::malformed());
    }
    Ok(match decode_turn(&result, request, runtime, usage) {
        Ok(turn) => Decoded::Turn(turn),
        Err(reason) => Decoded::Rejected { reason, usage, rejected: result },
    })
}

fn decode_turn(
    result: &str,
    request: &ModelRequest,
    runtime: &super::request::RuntimeRequest,
    usage: UsageCounters,
) -> Result<RuntimeTurn, TurnRejection> {
    let (json, repairs) = peritus_provider_core::healing::private_envelope(
        result,
        "opencode.turn",
        ProtocolLimits::PRODUCTION,
    )
    .map_err(|_| TurnRejection::Json)?
    .into_parts();
    let value: Value =
        serde_json::from_slice(json.canonical_bytes()).map_err(|_| TurnRejection::Json)?;
    let object = value.as_object().ok_or(TurnRejection::Envelope)?;
    if object.len() != 2 {
        return Err(TurnRejection::Envelope);
    }
    let content = match object.get("content") {
        Some(Value::String(text)) => text.clone(),
        // A task report can be a JSON object inside the private envelope. Serialize it
        // once here instead of requiring a model to escape nested JSON through two string
        // layers. Public report validation remains the caller's unchanged responsibility.
        Some(value @ Value::Object(_)) => {
            serde_json::to_string(value).map_err(|_| TurnRejection::Content)?
        }
        _ => return Err(TurnRejection::Content),
    };
    let calls = object.get("tool_calls").and_then(Value::as_array).ok_or(TurnRejection::Tools)?;
    if content.contains('\0') || content.is_empty() && calls.is_empty() {
        return Err(TurnRejection::Content);
    }
    if calls.len() > runtime.max_calls
        || calls.is_empty()
            && matches!(request.tool_choice(), ToolChoice::Required | ToolChoice::Specific(_))
    {
        return Err(TurnRejection::Tools);
    }
    let mut tool_calls = Vec::new();
    for call in calls {
        let call = call.as_object().ok_or(TurnRejection::Arguments)?;
        let name = call.get("name").and_then(Value::as_str).ok_or(TurnRejection::Tools)?;
        if call.len() != 2 || !runtime.allowed_tools.contains(name) {
            return Err(TurnRejection::Tools);
        }
        let arguments = call
            .get("arguments")
            .and_then(Value::as_object)
            .ok_or(TurnRejection::Arguments)?
            .clone();
        tool_calls.push(RuntimeToolCall { name: name.to_owned(), arguments });
    }
    Ok(RuntimeTurn { content, tool_calls, usage, repairs: repairs.into_iter().collect() })
}

fn identity(
    value: &Value,
    name: &str,
    expected: &mut Option<String>,
) -> Result<(), ProviderCoreError> {
    let actual = value
        .get(name)
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(super::malformed)?;
    if let Some(expected) = expected {
        if expected != actual {
            return Err(super::malformed());
        }
    } else {
        *expected = Some(actual.to_owned());
    }
    Ok(())
}

fn counters(part: &Value) -> Result<UsageCounters, ProviderCoreError> {
    let Some(tokens) = part.get("tokens") else {
        return Ok(UsageCounters::new(None, None, None, None, None, None, None, None));
    };
    let count = |name: &str| -> Result<Option<u64>, ProviderCoreError> {
        tokens.pointer(name).map(|value| value.as_u64().ok_or_else(super::malformed)).transpose()
    };
    let output = count("/output")?;
    let reasoning = count("/reasoning")?;
    let combined = match (output, reasoning) {
        (Some(output), Some(reasoning)) => {
            Some(output.checked_add(reasoning).ok_or_else(super::malformed)?)
        }
        (output, _) => output,
    };
    Ok(UsageCounters::new(
        count("/input")?,
        count("/cache/read")?,
        count("/cache/write")?,
        combined,
        reasoning,
        None,
        count("/total")?,
        None,
    ))
}

fn classify(event: &Value) -> NativeFailure {
    if event.pointer("/error/name").and_then(Value::as_str) == Some("ContextOverflowError") {
        return NativeFailure {
            category: FailureCategory::InvalidRequest,
            status: None,
            code: "opencode.runtime.input_limit",
        };
    }
    if event.pointer("/error/name").and_then(Value::as_str) == Some("ProviderModelNotFoundError") {
        return NativeFailure {
            category: FailureCategory::NotFound,
            status: None,
            code: "opencode.runtime.model_not_found",
        };
    }
    let data = event.pointer("/error/data");
    let status = data
        .and_then(|data| data.get("statusCode"))
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok());
    // Provider error bodies are never exposed: only recognized, fixed diagnostics leave C5.
    let message = data
        .and_then(|data| data.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let (category, code) = match status {
        Some(401) => (FailureCategory::Authentication, "opencode.runtime.authentication"),
        Some(403) if message.contains("subscription") => {
            (FailureCategory::Permission, "opencode.runtime.go_subscription_required")
        }
        Some(403) if message.contains("model access is disabled") => {
            (FailureCategory::Permission, "opencode.runtime.model_access_disabled")
        }
        Some(403) => (FailureCategory::Permission, "opencode.runtime.permission"),
        Some(404) => (FailureCategory::NotFound, "opencode.runtime.model_not_found"),
        Some(429)
            if message.contains("balance")
                || message.contains("quota")
                || message.contains("billing") =>
        {
            (FailureCategory::QuotaExhausted, "opencode.runtime.quota")
        }
        Some(429) => (FailureCategory::RateLimited, "opencode.runtime.rate_limit"),
        Some(400) if message.contains("go model requires global regions") => {
            (FailureCategory::InvalidRequest, "opencode.runtime.go_global_regions_required")
        }
        Some(400 | 413) if message.contains("context") || message.contains("token") => {
            (FailureCategory::InvalidRequest, "opencode.runtime.input_limit")
        }
        Some(400 | 422) => (FailureCategory::InvalidRequest, "opencode.runtime.invalid_request"),
        Some(500..=599) => (FailureCategory::TransientProvider, "opencode.runtime.unavailable"),
        _ => (FailureCategory::Provider, "opencode.runtime.provider_error"),
    };
    NativeFailure { category, status, code }
}

pub(super) fn observe_stderr(bytes: &[u8]) -> Option<ProviderCoreError> {
    // v1.19.0 incorrectly retries a 403 when its body says server_error. Observe only
    // recognized SDK error diagnostics; model text on stdout never enters this decision.
    let text = String::from_utf8_lossy(bytes);
    let diagnostic = if text
        .contains("AI_APICallError: Upstream request failed: This Go model requires Global regions")
    {
        "opencode.runtime.go_global_regions_required"
    } else if text.contains(
        "AI_APICallError: Upstream request failed: An active OpenCode Go subscription is required",
    ) {
        "opencode.runtime.go_subscription_required"
    } else if text.contains("AI_APICallError: Upstream request failed: Model access is disabled") {
        "opencode.runtime.model_access_disabled"
    } else if text.contains("AI_APICallError: Upstream request failed: Invalid credential") {
        "opencode.runtime.authentication"
    } else {
        return None;
    };
    Some(ProviderCoreError::configuration(
        diagnostic,
        "OpenCode rejected credentials or workspace/model access; repair the saved key or select an allowed model",
    ))
}

pub(super) fn observed_failure(error: &ProviderCoreError) -> Option<NativeFailure> {
    let code = match error.operation() {
        "opencode.runtime.go_subscription_required" => "opencode.runtime.go_subscription_required",
        "opencode.runtime.model_access_disabled" => "opencode.runtime.model_access_disabled",
        "opencode.runtime.authentication" => "opencode.runtime.authentication",
        "opencode.runtime.go_global_regions_required" => {
            "opencode.runtime.go_global_regions_required"
        }
        _ => return None,
    };
    Some(NativeFailure {
        category: if code == "opencode.runtime.go_global_regions_required" {
            FailureCategory::InvalidRequest
        } else if code == "opencode.runtime.authentication" {
            FailureCategory::Authentication
        } else {
            FailureCategory::Permission
        },
        status: None,
        code,
    })
}
