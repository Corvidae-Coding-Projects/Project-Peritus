//! Strict internal output schema for inert host tool proposals.

use std::collections::BTreeSet;

use peritus_model_protocol::{ModelRequest, ParallelToolPolicy, ToolChoice};
use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

use super::{invalid, object};

pub(super) struct ResultContract {
    pub bytes: Vec<u8>,
    pub allowed_tools: BTreeSet<String>,
    pub max_calls: usize,
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
    let name_schema = if names.is_empty() {
        object(vec![
            ("type", Value::String("string".to_owned())),
            ("const", Value::String("__no_host_tools__".to_owned())),
        ])
    } else {
        object(vec![
            ("type", Value::String("string".to_owned())),
            ("enum", Value::Array(names.iter().cloned().map(Value::String).collect())),
        ])
    };
    let item_schema = object(vec![
        ("type", Value::String("object".to_owned())),
        (
            "properties",
            object(vec![
                ("name", name_schema),
                (
                    "arguments_json",
                    object(vec![
                        ("type", Value::String("string".to_owned())),
                        (
                            "description",
                            Value::String(
                                "A JSON object encoded as text and matching the selected host tool schema"
                                    .to_owned(),
                            ),
                        ),
                    ]),
                ),
            ]),
        ),
        ("required", strings(&["name", "arguments_json"])),
        ("additionalProperties", Value::Bool(false)),
    ]);
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
    Ok(ResultContract { bytes, allowed_tools: names, max_calls: maximum })
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
