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
        if tool.parameters().dialect() != SchemaDialect::Draft202012 {
            return Err(error::invalid("compatible tools require JSON Schema 2020-12"));
        }
    }
    if let StructuredOutput::JsonSchema { schema, .. } = request.options().output()
        && schema.dialect() != SchemaDialect::Draft202012
    {
        return Err(error::invalid("compatible structured output requires JSON Schema 2020-12"));
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
            validate_block(block)?;
        }
    }
    Ok(())
}

fn validate_block(block: &ContentBlock) -> Result<(), ProviderCoreError> {
    match block {
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

pub(super) fn canonical(bytes: &[u8]) -> Result<Value, ProviderCoreError> {
    serde_json::from_slice(bytes)
        .map_err(|_| error::invalid("validated canonical JSON was not valid JSON"))
}
