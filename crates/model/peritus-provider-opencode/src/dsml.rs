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
    if !input.contains("<｜｜DSML｜｜")
        && !input.contains("<|｜DSML｜|")
        && !input.contains("<calls>")
    {
        return Ok(None);
    }
    if input.len() > peritus_provider_core::healing::MAX_REPAIR_BYTES {
        return Err(TurnRejection::Json);
    }
    let normalized = tags(input);
    let (open_tag, close_tag) = calls_tags(&normalized)?;
    let open = normalized.find(open_tag).ok_or(TurnRejection::Json)?;
    if normalized.matches(open_tag).count() != 1 || normalized.matches(close_tag).count() != 1 {
        return Err(TurnRejection::Json);
    }
    let close = normalized.find(close_tag).ok_or(TurnRejection::Json)?;
    if close < open || !normalized[close + close_tag.len()..].trim().is_empty() {
        return Err(TurnRejection::Json);
    }
    let content = normalized[..open].trim().to_owned();
    // A fenced example before the calls block is too easy to confuse with the intended call.
    if content.contains("```") {
        return Err(TurnRejection::Json);
    }
    let body = &normalized[open + open_tag.len()..close];
    let tool_calls = invokes(body)?;
    let tool_count = tool_calls.len();
    Ok(Some(NormalizedTurn { content, tool_calls, audit: audit(tool_count, input.len())? }))
}

fn tags(input: &str) -> String {
    input
        .replace("</｜｜DSML｜｜ ", "</")
        .replace("<｜｜DSML｜｜ ", "<")
        .replace("</|｜DSML｜| ", "</")
        .replace("<|｜DSML｜| ", "<")
        .replace("</｜｜DSML｜｜", "</")
        .replace("<｜｜DSML｜｜", "<")
        .replace("</|｜DSML｜|", "</")
        .replace("<|｜DSML｜|", "<")
}

fn calls_tags(input: &str) -> Result<(&'static str, &'static str), TurnRejection> {
    let calls = input.contains("<calls>");
    let tool_calls = input.contains("<tool_calls>");
    match (calls, tool_calls) {
        (true, false) => Ok(("<calls>", "</calls>")),
        (false, true) => Ok(("<tool_calls>", "</tool_calls>")),
        _ => Err(TurnRejection::Json),
    }
}

fn invokes(mut body: &str) -> Result<Vec<RuntimeToolCall>, TurnRejection> {
    let mut calls = Vec::new();
    loop {
        body = body.trim_start();
        if body.is_empty() {
            break;
        }
        body = body.strip_prefix("<invoke name=\"").ok_or(TurnRejection::Json)?;
        let (name, after_name) = body.split_once("\">").ok_or(TurnRejection::Json)?;
        let name = name.trim();
        if name.is_empty() || name.contains('<') || name.contains('>') {
            return Err(TurnRejection::Json);
        }
        let (arguments, after_invoke) =
            after_name.split_once("</invoke>").ok_or(TurnRejection::Json)?;
        body = after_invoke;
        let mut arguments = parameters(arguments)?;
        flatten_nested(name, &mut arguments);
        calls.push(RuntimeToolCall { name: name.to_owned(), arguments });
    }
    if calls.is_empty() { Err(TurnRejection::Json) } else { Ok(calls) }
}

fn parameters(mut body: &str) -> Result<Map<String, Value>, TurnRejection> {
    let mut arguments = Map::new();
    loop {
        body = body.trim_start();
        if body.is_empty() {
            break;
        }
        body = body.strip_prefix("<parameter name=\"").ok_or(TurnRejection::Json)?;
        let (name, after_name) = body.split_once("\" string=\"").ok_or(TurnRejection::Json)?;
        let (kind, after_kind) = after_name.split_once("\">").ok_or(TurnRejection::Json)?;
        let is_string = match kind {
            "true" => true,
            "false" => false,
            _ => return Err(TurnRejection::Json),
        };
        let (value, after_value) =
            after_kind.split_once("</parameter>").ok_or(TurnRejection::Json)?;
        body = after_value;
        if name.is_empty() || arguments.contains_key(name) {
            return Err(TurnRejection::Json);
        }
        let value = if is_string {
            Value::String(value.to_owned())
        } else {
            serde_json::from_str(value).map_err(|_| TurnRejection::Json)?
        };
        arguments.insert(name.to_owned(), value);
    }
    Ok(arguments)
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
