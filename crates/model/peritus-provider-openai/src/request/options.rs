//! Tool, structured-output, reasoning, sampling, and cache projection.

use peritus_model_protocol::{
    CachePolicy, ModelRequest, ParallelToolPolicy, ReasoningEffort, ReasoningPolicy, SchemaDialect,
    StructuredOutput, SummaryPolicy, ToolChoice,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

use super::value::{object, optional_string, string};
use crate::error;

pub(super) fn validate(request: &ModelRequest) -> Result<(), ProviderCoreError> {
    for tool in request.tools() {
        if tool.parameters().dialect() != SchemaDialect::Draft202012 {
            return Err(error::invalid("OpenAI function schemas must use JSON Schema 2020-12"));
        }
    }
    if let StructuredOutput::JsonSchema { schema, .. } = request.options().output()
        && schema.dialect() != SchemaDialect::Draft202012
    {
        return Err(error::invalid(
            "OpenAI structured output schemas must use JSON Schema 2020-12",
        ));
    }
    if let ParallelToolPolicy::Allowed(maximum) = request.parallel_tool_policy()
        && maximum != request.negotiated().limits().max_parallel_tool_calls()
    {
        return Err(error::invalid(
            "OpenAI parallel tool calls cannot enforce a narrower per-request count",
        ));
    }
    if matches!(request.options().cache(), CachePolicy::Ephemeral { ttl_seconds } if *ttl_seconds != 1_800)
    {
        return Err(error::invalid("OpenAI prompt-cache TTL must be exactly 30 minutes"));
    }
    Ok(())
}

pub(super) fn tools(request: &ModelRequest) -> Result<Vec<Value>, ProviderCoreError> {
    request
        .tools()
        .iter()
        .map(|tool| {
            Ok(object([
                (
                    "description",
                    optional_string(
                        tool.description()
                            .map(peritus_model_protocol::BoundedText::expose_for_wire),
                    ),
                ),
                ("name", string(tool.name().as_str())),
                ("parameters", canonical(tool.parameters().canonical_bytes())?),
                ("strict", Value::Bool(tool.strict())),
                ("type", string("function")),
            ]))
        })
        .collect()
}

pub(super) fn tool_choice(request: &ModelRequest) -> Option<Value> {
    if request.tools().is_empty() && matches!(request.tool_choice(), ToolChoice::Auto) {
        return None;
    }
    Some(match request.tool_choice() {
        ToolChoice::Auto => string("auto"),
        ToolChoice::None => string("none"),
        ToolChoice::Required => string("required"),
        ToolChoice::Specific(name) => {
            object([("name", string(name.as_str())), ("type", string("function"))])
        }
    })
}

pub(super) const fn parallel(request: &ModelRequest) -> bool {
    matches!(request.parallel_tool_policy(), ParallelToolPolicy::Allowed(_))
}

pub(super) fn text(request: &ModelRequest) -> Result<Value, ProviderCoreError> {
    let format = match request.options().output() {
        StructuredOutput::Text => object([("type", string("text"))]),
        StructuredOutput::JsonObject => object([("type", string("json_object"))]),
        StructuredOutput::JsonSchema { name, schema, strict } => object([
            ("name", string(name.as_str())),
            ("schema", canonical(schema.canonical_bytes())?),
            ("strict", Value::Bool(*strict)),
            ("type", string("json_schema")),
        ]),
    };
    Ok(object([("format", format)]))
}

pub(super) fn reasoning(request: &ModelRequest) -> Option<Value> {
    match request.options().reasoning() {
        ReasoningPolicy::Disabled => None,
        ReasoningPolicy::Adaptive { summary } => {
            Some(object([("summary", optional_string(summary_name(summary)))]))
        }
        ReasoningPolicy::Effort { effort, summary } => Some(object([
            ("effort", string(effort_name(effort))),
            ("summary", optional_string(summary_name(summary))),
        ])),
    }
}

pub(super) fn cache_key(request: &ModelRequest) -> Option<&str> {
    match request.options().cache() {
        CachePolicy::Explicit(key) => Some(key.expose_for_wire()),
        CachePolicy::Disabled | CachePolicy::Automatic | CachePolicy::Ephemeral { .. } => None,
    }
}

pub(super) fn cache_options(request: &ModelRequest) -> Option<Value> {
    matches!(request.options().cache(), CachePolicy::Ephemeral { .. })
        .then(|| object([("ttl", string("30m"))]))
}

const fn effort_name(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::Minimal => "minimal",
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
    }
}

const fn summary_name(summary: SummaryPolicy) -> Option<&'static str> {
    match summary {
        SummaryPolicy::None => None,
        SummaryPolicy::Auto => Some("auto"),
        SummaryPolicy::Concise => Some("concise"),
        SummaryPolicy::Detailed => Some("detailed"),
    }
}

fn canonical(bytes: &[u8]) -> Result<Value, ProviderCoreError> {
    serde_json::from_slice(bytes).map_err(|_| error::invalid("validated schema JSON was invalid"))
}
