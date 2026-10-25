//! Redaction-safe classification of official Codex runtime failure families.

use serde_json::Value;

use super::DecodeFailure;

/// Codex can reject turn/start before emitting a JSONL error event. Read only the
/// structured diagnostic code, never forwarding raw process stderr to the model.
pub fn reported_process_failure(stderr: &[u8]) -> Option<DecodeFailure> {
    std::str::from_utf8(stderr).ok()?.lines().rev().find_map(|line| {
        let (_, data) = line.split_once("data: ")?;
        let value: Value = serde_json::from_str(data).ok()?;
        (value.get("input_error_code").and_then(Value::as_str) == Some("input_too_large"))
            .then_some(DecodeFailure::InputLimit)
    })
}

pub(super) fn reported_failure(event: &Value) -> DecodeFailure {
    // The official executable may wrap the service's structured error in its
    // message string. Unwrap one level without forwarding private error text.
    let nested = event
        .pointer("/error/message")
        .or_else(|| event.get("message"))
        .and_then(Value::as_str)
        .and_then(|message| serde_json::from_str::<Value>(message).ok());
    let event = nested.as_ref().filter(|value| value.is_object()).unwrap_or(event);
    let code = event
        .pointer("/error/code")
        .or_else(|| event.pointer("/error/type"))
        .or_else(|| event.pointer("/error/codexErrorInfo"))
        .or_else(|| event.get("code"))
        .and_then(Value::as_str);
    if matches!(code, Some("authentication_error" | "unauthorized" | "invalid_api_key")) {
        return DecodeFailure::Authentication;
    }
    let message = event
        .pointer("/error/message")
        .or_else(|| event.get("message"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        code,
        Some("cyberPolicy" | "cyber_policy" | "misalignment_policy_violation" | "content_policy")
    ) || message.contains("cyber policy")
        || message.contains("misalignment policy")
        || message.contains("content policy")
        || message.contains("safety policy")
    {
        DecodeFailure::Safety
    } else if message.contains("at capacity")
        || message.contains("model capacity")
        || message.contains("temporarily overloaded")
    {
        DecodeFailure::Capacity
    } else if matches!(code, Some("rate_limit_exceeded" | "rate_limited"))
        || message.contains("rate limit")
        || message.contains("too many requests")
    {
        DecodeFailure::RateLimited
    } else if matches!(code, Some("insufficient_quota" | "billing_hard_limit_reached"))
        || message.contains("quota")
        || message.contains("billing limit")
    {
        DecodeFailure::QuotaExhausted
    } else if matches!(code, Some("model_not_found" | "model_not_supported" | "unsupported_model"))
        || message.contains("model is not supported")
        || message.contains("model is not available")
        || message.contains("do not have access to this model")
        || message.contains("do not have access to the model")
    {
        DecodeFailure::ModelUnavailable
    } else if matches!(code, Some("context_length_exceeded"))
        || message.contains("context window")
        || message.contains("context length")
    {
        DecodeFailure::ContextLimit
    } else {
        DecodeFailure::Reported
    }
}
