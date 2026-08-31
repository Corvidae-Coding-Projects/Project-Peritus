//! Model-visible bounds for exact tool observations retained in the durable trace.

use core::fmt::Write as _;

use peritus_model_protocol::{CanonicalJson, JsonBounds, ProtocolLimits};
use serde_json::{Map, Value};

use super::DeveloperLoopError;

const POLICY: &str = "peritus-tool-output-v1";
const MAX_TOKENS: u64 = 10_000;
const CONTEXT_FRACTION: u64 = 8;
const MIN_TOKENS: u64 = 512;
const BYTES_PER_TOKEN: usize = 3;

/// Retains exact output in the caller-owned trace while bounding the copy admitted to model history.
pub(super) fn model_visible_tool_output(
    output: &CanonicalJson,
    provider_input_tokens: u64,
    limits: ProtocolLimits,
) -> Result<CanonicalJson, DeveloperLoopError> {
    let token_budget = (provider_input_tokens / CONTEXT_FRACTION).clamp(MIN_TOKENS, MAX_TOKENS);
    let byte_budget = usize::try_from(token_budget)
        .unwrap_or(usize::MAX)
        .saturating_mul(BYTES_PER_TOKEN)
        .min(JsonBounds::value(limits).max_bytes());
    let exact = output.canonical_bytes();
    if exact.len() <= byte_budget {
        return Ok(output.clone());
    }

    let text = output.to_wire_string();
    let original_digest = digest_hex(output.digest());
    let original_token_estimate = exact.len().div_ceil(BYTES_PER_TOKEN);
    let mut preview_chars = byte_budget / 8;
    loop {
        let (head, tail) = head_tail(&text, preview_chars);
        let mut details = Map::new();
        details.insert("head".to_owned(), Value::String(head));
        details.insert("original_bytes".to_owned(), Value::from(exact.len()));
        details.insert("original_sha256".to_owned(), Value::String(original_digest.clone()));
        details.insert("original_token_estimate".to_owned(), Value::from(original_token_estimate));
        details.insert("policy".to_owned(), Value::String(POLICY.to_owned()));
        details.insert("tail".to_owned(), Value::String(tail));
        details.insert(
            "warning".to_owned(),
            Value::String(
                "Tool output was truncated before model context. The exact output remains in the run trace; use a narrower tool request for omitted detail."
                    .to_owned(),
            ),
        );
        let mut root = Map::new();
        root.insert("peritus_truncated_tool_output".to_owned(), Value::Object(details));
        let value = Value::Object(root);
        let rendered = serde_json::to_string(&value).map_err(|_| {
            DeveloperLoopError::Context("tool observation truncation could not encode JSON".into())
        })?;
        if rendered.len() <= byte_budget {
            return Ok(CanonicalJson::parse(&rendered, JsonBounds::value(limits))?);
        }
        if preview_chars == 0 {
            return Err(DeveloperLoopError::Context(
                "tool observation truncation metadata exceeded its model-visible budget".into(),
            ));
        }
        preview_chars /= 2;
    }
}

fn head_tail(value: &str, chars: usize) -> (String, String) {
    let head = value.chars().take(chars).collect();
    let mut tail = value.chars().rev().take(chars).collect::<Vec<_>>();
    tail.reverse();
    (head, tail.into_iter().collect())
}

fn digest_hex(digest: peritus_types::Sha256Digest) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest.as_bytes() {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
