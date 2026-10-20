//! Strict normalization of the DSML tool-call markup `DeepSeek` sometimes emits.

use peritus_model_protocol::{
    CanonicalJson, ExtensionName, JsonBounds, ModelEvent, ProtocolLimits, ProviderExtension,
};
use serde_json::{Map, Value};

use super::output::RuntimeToolCall;
use super::recovery::TurnRejection;

pub(super) struct NormalizedTurn {
    pub content: String,
    pub tool_calls: Vec<RuntimeToolCall>,
    pub audit: ModelEvent,
}

/// Converts one complete, terminal DSML calls block into the private turn shape.
///
/// Returns `Ok(None)` when the reply does not contain the markup. Any recognized block that is
/// incomplete, ambiguous, or not the final payload is refused rather than partially interpreted.
pub(super) fn normalize(input: &str) -> Result<Option<NormalizedTurn>, TurnRejection> {
    if input.len() > peritus_provider_core::healing::MAX_REPAIR_BYTES {
        return Err(TurnRejection::Json);
    }
    for (offset, _) in input.match_indices('<') {
        for root in ["calls", "tool_calls"] {
            if let Some((body, style)) = open(&input[offset..], &format!("{root}>")) {
                let content = input[..offset].trim();
                // Like the JSON-envelope prelude, prose must not contain another structured
                // candidate. Never reinterpret an earlier envelope as explanatory text.
                if content.contains(['{', '}', '[', ']']) || content.contains("```") {
                    return Err(TurnRejection::Json);
                }
                let tool_calls = invokes(body, root, style)?;
                let audit = audit(tool_calls.len(), input.len())?;
                return Ok(Some(NormalizedTurn { content: content.to_owned(), tool_calls, audit }));
            }
        }
    }
    if input.contains("DSML") || input.contains("<calls") || input.contains("<tool_calls") {
        Err(TurnRejection::Json)
    } else {
        Ok(None)
    }
}

/// Structural markers are recognized only at the parser cursor, never rewritten in values.
fn open<'a>(input: &'a str, expected: &str) -> Option<(&'a str, &'static str)> {
    let input = input.strip_prefix('<')?;
    for marker in ["｜｜DSML｜｜", "|｜DSML｜|", ""] {
        if let Some(body) = input.strip_prefix(marker) {
            let body =
                if marker.is_empty() { body } else { body.strip_prefix(' ').unwrap_or(body) };
            if let Some(rest) = body.strip_prefix(expected) {
                return Some((rest, marker));
            }
        }
    }
    None
}

fn close<'a>(input: &'a str, name: &str, marker: &str) -> Option<&'a str> {
    let body = input.strip_prefix("</")?.strip_prefix(marker)?;
    let body = if marker.is_empty() { body } else { body.strip_prefix(' ').unwrap_or(body) };
    body.strip_prefix(name)?.strip_prefix('>')
}

fn invokes(mut body: &str, root: &str, style: &str) -> Result<Vec<RuntimeToolCall>, TurnRejection> {
    let mut calls = Vec::new();
    loop {
        body = body.trim_start();
        if let Some(rest) = close(body, root, style) {
            if !rest.trim().is_empty() {
                return Err(TurnRejection::Json);
            }
            break;
        }
        let (after_open, invoke_style) = open(body, "invoke name=\"").ok_or(TurnRejection::Json)?;
        body = after_open;
        let (name, after_name) = body.split_once("\">").ok_or(TurnRejection::Json)?;
        let name = name.trim();
        if name.is_empty() || name.contains('<') || name.contains('>') {
            return Err(TurnRejection::Json);
        }
        let (mut arguments, after_invoke) = parameters(after_name, invoke_style)?;
        body = after_invoke;
        flatten_nested(name, &mut arguments);
        calls.push(RuntimeToolCall { name: name.to_owned(), arguments });
    }
    if calls.is_empty() { Err(TurnRejection::Json) } else { Ok(calls) }
}

fn parameters<'a>(
    mut body: &'a str,
    style: &str,
) -> Result<(Map<String, Value>, &'a str), TurnRejection> {
    let mut arguments = Map::new();
    loop {
        body = body.trim_start();
        if let Some(rest) = close(body, "invoke", style) {
            return Ok((arguments, rest));
        }
        let (after_open, parameter_style) =
            open(body, "parameter name=\"").ok_or(TurnRejection::Json)?;
        body = after_open;
        let (name, after_name) = body.split_once("\" string=\"").ok_or(TurnRejection::Json)?;
        let (kind, after_kind) = after_name.split_once("\">").ok_or(TurnRejection::Json)?;
        let is_string = match kind {
            "true" => true,
            "false" => false,
            _ => return Err(TurnRejection::Json),
        };
        let (offset, after_value) = after_kind
            .match_indices("</")
            .find_map(|(offset, _)| {
                close(&after_kind[offset..], "parameter", parameter_style)
                    .map(|rest| (offset, rest))
            })
            .ok_or(TurnRejection::Json)?;
        let value = &after_kind[..offset];
        body = after_value;
        if name.is_empty() || arguments.contains_key(name) {
            return Err(TurnRejection::Json);
        }
        let value = if is_string {
            Value::String(value.to_owned())
        } else {
            let canonical =
                CanonicalJson::parse(value, JsonBounds::value(ProtocolLimits::PRODUCTION))
                    .map_err(|_| TurnRejection::Json)?;
            serde_json::from_slice(canonical.canonical_bytes()).map_err(|_| TurnRejection::Json)?
        };
        arguments.insert(name.to_owned(), value);
    }
}

fn flatten_nested(name: &str, arguments: &mut Map<String, Value>) {
    if arguments.len() != 2 || arguments.get("name").and_then(Value::as_str) != Some(name) {
        return;
    }
    if let Some(Value::Object(nested)) = arguments.get("arguments") {
        *arguments = nested.clone();
    }
}

fn audit(tool_calls: usize, original_bytes: usize) -> Result<ModelEvent, TurnRejection> {
    let value = serde_json::json!({
        "policy": "dsml-v1",
        "target": "opencode.turn",
        "tool_calls": tool_calls,
        "original_bytes": original_bytes,
        "tools_executed": false,
    });
    let value =
        CanonicalJson::parse(&value.to_string(), JsonBounds::value(ProtocolLimits::PRODUCTION))
            .map_err(|_| TurnRejection::Json)?;
    let name = ExtensionName::new("peritus.opencode_turn_normalization".to_owned())
        .map_err(|_| TurnRejection::Json)?;
    Ok(ModelEvent::ProviderEvent(ProviderExtension::new(name, value)))
}
