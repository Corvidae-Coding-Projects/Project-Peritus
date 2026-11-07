//! Strict internal output schema for inert host tool proposals.

use std::collections::BTreeSet;

use peritus_model_protocol::{ModelRequest, ParallelToolPolicy, ToolArgumentSchema, ToolChoice};
use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

use super::{invalid, object};

pub(super) struct ResultContract {
    pub bytes: Vec<u8>,
    pub allowed_tools: BTreeSet<String>,
    pub max_calls: usize,
    pub min_calls: usize,
}

pub(super) fn result_contract(request: &ModelRequest) -> Result<ResultContract, ProviderCoreError> {
    let selected = match request.tool_choice() {
        ToolChoice::Specific(name) => Some(name.as_str()),
        ToolChoice::Auto | ToolChoice::None | ToolChoice::Required => None,
    };
    let mut names = BTreeSet::new();
    let mut available = 0_usize;
    for tool in request
        .tools()
        .iter()
        .filter(|tool| selected.is_none_or(|name| name == tool.name().as_str()))
    {
        if !names.insert(tool.name().as_str().to_owned()) {
            return Err(invalid("Codex runtime tool names must be unique"));
        }
        available = available.checked_add(1).ok_or_else(|| invalid("tool count overflowed"))?;
    }
    if selected.is_some() && available == 0 {
        return Err(invalid("Codex runtime selected tool is absent"));
    }
    let maximum = max_calls(request, available)?;
    let minimum = usize::from(matches!(
        request.tool_choice(),
        ToolChoice::Required | ToolChoice::Specific(_)
    ));
    let mut variants = Vec::new();
    for tool in request.tools().iter().filter(|tool| names.contains(tool.name().as_str())) {
        let contract = ToolArgumentSchema::compile(tool.parameters()).map_err(|_| {
            invalid("unsupported host tool schema; host admission reports field diagnostics")
        })?;
        // Unsupported strict projections remain explicitly host-validated.
        let arguments = contract.provider_schema().unwrap_or_else(
            |_| serde_json::json!({"type":"string","description":"JSON object encoded once; the host validates the selected tool schema."})
        );
        // The tool key is selected before its argument value, regardless of JSON map
        // ordering in the CLI or app-server. A late name discriminator lets constrained
        // decoding choose an argument shape first and then force an unintended tool.
        variants.push(serde_json::json!({"type":"object","properties":{tool.name().as_str():arguments},"required":[tool.name().as_str()],"additionalProperties":false}));
    }
    let item_schema = if variants.is_empty() {
        serde_json::json!({"type":"object","properties":{"__no_host_tools__":{"type":"null"}},"required":["__no_host_tools__"],"additionalProperties":false})
    } else {
        serde_json::json!({"anyOf":variants})
    };
    let tool_calls = object(vec![
        ("type", Value::String("array".to_owned())),
        ("minItems", Value::Number(minimum.into())),
        ("maxItems", Value::Number(maximum.into())),
        ("items", item_schema),
    ]);
    let schema = object(vec![
        ("type", Value::String("object".to_owned())),
        (
            "properties",
            object(vec![
                ("content", object(vec![("type", Value::String("string".to_owned()))])),
                ("tool_calls", tool_calls),
            ]),
        ),
        ("required", strings(&["content", "tool_calls"])),
        ("additionalProperties", Value::Bool(false)),
    ]);
    let bytes = serde_json::to_vec(&schema)
        .map_err(|_| invalid("Codex runtime output schema serialization failed"))?;
    Ok(ResultContract { bytes, allowed_tools: names, min_calls: minimum, max_calls: maximum })
}

fn max_calls(request: &ModelRequest, variants: usize) -> Result<usize, ProviderCoreError> {
    match (request.tool_choice(), request.parallel_tool_policy()) {
        (ToolChoice::None, _) => Ok(0),
        (_, ParallelToolPolicy::Disabled) => Ok(usize::from(variants > 0)),
        (_, ParallelToolPolicy::Allowed(count)) => usize::try_from(count)
            .map_err(|_| invalid("parallel tool-call limit is not representable")),
    }
}

fn strings(values: &[&str]) -> Value {
    Value::Array(values.iter().map(|value| Value::String((*value).to_owned())).collect())
}
