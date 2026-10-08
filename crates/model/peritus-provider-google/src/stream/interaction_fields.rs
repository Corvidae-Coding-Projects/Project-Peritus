//! Bounded field helpers for the Interactions streaming grammar.

use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

use super::value::invalid;

pub(super) fn summary_texts(value: &Value) -> Result<Vec<&str>, ProviderCoreError> {
    let content = value
        .pointer("/delta/content")
        .ok_or_else(|| invalid("Google thought summary content is missing"))?;
    if let Some(text) = content.get("text").and_then(Value::as_str) {
        return Ok(vec![text]);
    }
    let items = content
        .as_array()
        .filter(|items| !items.is_empty())
        .ok_or_else(|| invalid("Google thought summary is not text"))?;
    items
        .iter()
        .map(|item| {
            item.get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("Google thought summary member is not text"))
        })
        .collect()
}

pub(super) fn correctness_critical(kind: &str) -> bool {
    kind.starts_with("interaction.") || kind.starts_with("step.")
}
