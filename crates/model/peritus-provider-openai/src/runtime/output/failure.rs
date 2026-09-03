//! Redaction-safe classification of official Codex runtime failure families.

use serde_json::Value;

use super::DecodeFailure;

pub(super) fn reported_failure(event: &Value) -> DecodeFailure {
    let code = event
        .pointer("/error/type")
        .or_else(|| event.pointer("/error/code"))
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
    if matches!(code, Some("cyber_policy" | "misalignment_policy_violation" | "content_policy"))
        || message.contains("cyber policy")
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
    } else if matches!(code, Some("context_length_exceeded"))
        || message.contains("context window")
        || message.contains("context length")
    {
        DecodeFailure::ContextLimit
    } else {
        DecodeFailure::Reported
    }
}
