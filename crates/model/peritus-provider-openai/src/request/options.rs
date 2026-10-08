//! Tool, structured-output, sampling, and cache admission.

use peritus_model_protocol::{
    CachePolicy, ContentBlock, ModelRequest, ParallelToolPolicy, SchemaDialect, StructuredOutput,
};
use peritus_provider_core::ProviderCoreError;

use crate::error;

pub(super) fn validate(request: &ModelRequest) -> Result<(), ProviderCoreError> {
    for tool in request.tools() {
        if !valid_name(tool.name().as_str()) {
            return Err(error::invalid(
                "OpenAI function names must use at most 64 ASCII letters, digits, underscores, or dashes",
            ));
        }
        if tool.parameters().dialect() != SchemaDialect::Draft202012 {
            return Err(error::invalid("OpenAI function schemas must use JSON Schema 2020-12"));
        }
    }
    if let StructuredOutput::JsonSchema { name, schema, .. } = request.options().output() {
        if !valid_name(name.as_str()) {
            return Err(error::invalid(
                "OpenAI output names must use at most 64 ASCII letters, digits, underscores, or dashes",
            ));
        }
        if schema.dialect() != SchemaDialect::Draft202012 {
            return Err(error::invalid(
                "OpenAI structured output schemas must use JSON Schema 2020-12",
            ));
        }
    }
    if request.messages().iter().flat_map(|message| message.content()).any(|block| {
        matches!(block, ContentBlock::ToolCall(call) if !valid_name(call.name().as_str()))
    }) {
        return Err(error::invalid(
            "OpenAI replayed function names must use the Responses function-name grammar",
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
    if matches!(request.options().cache(), CachePolicy::Explicit(key) if key.expose_for_wire().len() > 64)
    {
        return Err(error::invalid("OpenAI prompt-cache keys must be at most 64 characters"));
    }
    Ok(())
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
