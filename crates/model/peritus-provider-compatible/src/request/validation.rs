use peritus_model_protocol::{
    CachePolicy, Capability, ContentBlock, MediaKind, MediaReferenceKind, ModelRequest,
    ParallelToolPolicy, ReasoningPolicy, SchemaDialect, StateMode, StructuredOutput,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

use crate::{error, profile::CompatibleProfile};

pub(super) fn validate(
    profile: &CompatibleProfile,
    request: &ModelRequest,
    service: Option<peritus_provider_core::hosted::HostedService>,
) -> Result<(), ProviderCoreError> {
    if !request.negotiated().includes(Capability::Streaming) {
        return Err(error::invalid("compatible streaming must be explicitly negotiated"));
    }
    if profile.provider_profile().state_mode() != StateMode::StatelessReplay
        || request.options().persistence().store()
        || request.options().persistence().background()
        || request.options().continuation().is_some()
    {
        return Err(error::invalid("compatible profiles do not map persistence or continuation"));
    }
    if !matches!(request.options().cache(), CachePolicy::Disabled)
        || !matches!(request.options().reasoning(), ReasoningPolicy::Disabled)
    {
        return Err(error::invalid("compatible profiles do not map cache or reasoning controls"));
    }
    for tool in request.tools() {
        if !valid_name(tool.name().as_str()) {
            return Err(error::invalid(
                "compatible function names must use at most 64 ASCII letters, digits, underscores, or dashes",
            ));
        }
        if tool.parameters().dialect() != SchemaDialect::Draft202012 {
            return Err(error::invalid("compatible tools require JSON Schema 2020-12"));
        }
    }
    if let StructuredOutput::JsonSchema { name, schema, .. } = request.options().output() {
        if !valid_name(name.as_str()) {
            return Err(error::invalid(
                "compatible output names must use at most 64 ASCII letters, digits, underscores, or dashes",
            ));
        }
        if schema.dialect() != SchemaDialect::Draft202012 {
            return Err(error::invalid(
                "compatible structured output requires JSON Schema 2020-12",
            ));
        }
    }
    if let ParallelToolPolicy::Allowed(maximum) = request.parallel_tool_policy()
        && maximum != request.negotiated().limits().max_parallel_tool_calls()
    {
        return Err(error::invalid(
            "compatible parallel tools cannot enforce an unmapped narrower count",
        ));
    }
    for message in request.messages() {
        for block in message.content() {
            if let ContentBlock::Reasoning(replay) = block
                && service.is_some()
                && profile.provider_profile().dialect()
                    == peritus_model_protocol::WireDialect::CompatibleChatCompletions
            {
                super::hosted::replay(replay, service)?;
            } else {
                validate_block(block)?;
            }
        }
    }
    Ok(())
}

fn validate_block(block: &ContentBlock) -> Result<(), ProviderCoreError> {
    match block {
        ContentBlock::ToolCall(call) if !valid_name(call.name().as_str()) => Err(error::invalid(
            "compatible replayed function name violates the selected wire contract",
        )),
        ContentBlock::Image(media) if media.kind() != MediaKind::Image => {
            Err(error::invalid("compatible image block has another media kind"))
        }
        ContentBlock::Image(media) => match media.reference_for_wire() {
            Some((MediaReferenceKind::HttpsUrl, _)) | None
                if media.inline_bytes_for_wire().is_some() =>
            {
                Ok(())
            }
            Some((MediaReferenceKind::HttpsUrl, _)) => Ok(()),
            Some((MediaReferenceKind::ProviderFile, _)) => Err(error::invalid(
                "compatible image mappings do not assume provider file identities",
            )),
            None => {
                Err(error::invalid("compatible artifact media must be resolved before transport"))
            }
        },
        ContentBlock::Audio(_) | ContentBlock::Document(_) => {
            Err(error::invalid("compatible profiles do not map audio or document input"))
        }
        ContentBlock::Reasoning(_) => {
            Err(error::invalid("compatible profiles do not map reasoning replay"))
        }
        ContentBlock::ProviderExtension(_) => {
            Err(error::invalid("compatible profiles do not map provider extensions"))
        }
        ContentBlock::Text(_)
        | ContentBlock::ToolCall(_)
        | ContentBlock::ToolResult(_)
        | ContentBlock::Refusal(_) => Ok(()),
    }
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub(super) fn canonical(bytes: &[u8]) -> Result<Value, ProviderCoreError> {
    serde_json::from_slice(bytes)
        .map_err(|_| error::invalid("validated canonical JSON was not valid JSON"))
}
