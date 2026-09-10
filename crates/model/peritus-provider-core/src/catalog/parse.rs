//! Strict bounded catalog projections shared by HTTP and official account protocols.

use serde_json::Value;

use super::{DiscoveredModel, MAX_CATALOG_MODELS, unavailable};
use crate::ProviderCoreError;

pub(super) fn model(value: &Value, google: bool) -> Result<DiscoveredModel, ProviderCoreError> {
    let id = if google {
        value.get("name")
    } else {
        value.get("model").or_else(|| value.get("value")).or_else(|| value.get("id"))
    }
    .and_then(Value::as_str)
    .ok_or_else(|| unavailable("catalog model has no identifier"))?;
    let id = if google { id.strip_prefix("models/").unwrap_or(id) } else { id };
    let label = value
        .get("display_name")
        .or_else(|| value.get("displayName"))
        .or_else(|| value.get("label"))
        .and_then(Value::as_str)
        .unwrap_or(id);
    let mut model = DiscoveredModel::new(id.to_owned(), label.to_owned())?;
    model.tools = value
        .pointer("/capabilities/tool_use/supported")
        .and_then(Value::as_bool)
        .or_else(|| value.get("supports_tools").and_then(Value::as_bool))
        .or_else(|| value.get("supportsTools").and_then(Value::as_bool))
        .or_else(|| {
            value.get("supported_parameters").and_then(Value::as_array).map(|parameters| {
                parameters.iter().any(|parameter| parameter.as_str() == Some("tools"))
            })
        });
    if let Some(methods) = value.get("supportedGenerationMethods").and_then(Value::as_array)
        && !methods.iter().any(|method| method.as_str() == Some("generateContent"))
    {
        model.tools = Some(false);
    }
    model.input_tokens = value
        .get("inputTokenLimit")
        .or_else(|| value.get("max_input_tokens"))
        .or_else(|| value.get("context_window"))
        .or_else(|| value.get("context_length"))
        .or_else(|| value.get("contextLength"))
        .and_then(Value::as_u64)
        .filter(|value| *value > 0);
    model.output_tokens = value
        .get("outputTokenLimit")
        .or_else(|| value.get("max_tokens"))
        .or_else(|| value.get("max_completion_tokens"))
        .or_else(|| value.pointer("/top_provider/max_completion_tokens"))
        .and_then(Value::as_u64)
        .filter(|value| *value > 0);
    Ok(model)
}

/// Parses an official runtime model-list array, rejecting incomplete or oversized results.
///
/// # Errors
/// Returns a safe error for malformed model entries or collections.
pub(super) fn parse_runtime_models(
    value: &Value,
) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
    let values = value.as_array().ok_or_else(|| unavailable("runtime returned no model array"))?;
    if values.len() > MAX_CATALOG_MODELS {
        return Err(unavailable("runtime model catalog exceeds its bound"));
    }
    values.iter().map(|value| model(value, false)).collect()
}
