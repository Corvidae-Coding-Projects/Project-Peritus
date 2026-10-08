use peritus_model_protocol::{
    CachePolicy, Capability, ContentBlock, MediaKind, MediaReferenceKind, ModelRequest,
    ParallelToolPolicy, ReasoningPolicy, SchemaDialect, StructuredOutput, ToolChoice,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

use crate::{error, profile::{CompatibleProfile, RequestField}};

pub(super) fn validate(
    profile: &CompatibleProfile,
    request: &ModelRequest,
    service: Option<peritus_provider_core::hosted::HostedService>,
) -> Result<(), ProviderCoreError> {
    if !request.negotiated().includes(Capability::Streaming) {
        return Err(error::invalid("compatible streaming must be explicitly negotiated"));
    }
    validate_operations(profile, request)?;
    if request.options().persistence().background() {
        return Err(error::invalid("compatible profiles do not map background execution"));
    }
    if !matches!(request.options().cache(), CachePolicy::Disabled)
        || !matches!(request.options().reasoning(), ReasoningPolicy::Disabled)
        || !request.options().extensions().is_empty()
    {
        return Err(error::invalid(
            "compatible profiles do not map cache, reasoning controls, or provider extensions",
        ));
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
                && profile.contract().supports(RequestField::ReasoningReplay)
            {
                super::hosted::replay(replay, service)?;
            } else {
                validate_block(block)?;
            }
        }
    }
    Ok(())
}

fn validate_operations(
    profile: &CompatibleProfile,
    request: &ModelRequest,
) -> Result<(), ProviderCoreError> {
    let blocks = || request.messages().iter().flat_map(peritus_model_protocol::Message::content);
    let uses_tools = !request.tools().is_empty()
        || !matches!(request.tool_choice(), ToolChoice::Auto)
        || blocks().any(|block| {
            matches!(block, ContentBlock::ToolCall(_) | ContentBlock::ToolResult(_))
        });
    require_capability_mapping(
        profile,
        request,
        uses_tools,
        Capability::ToolCalls,
        RequestField::Tools,
        "compatible request uses tools without a negotiated implemented mapping",
    )?;
    require_capability_mapping(
        profile,
        request,
        matches!(request.parallel_tool_policy(), ParallelToolPolicy::Allowed(_)),
        Capability::ParallelToolCalls,
        RequestField::ParallelTools,
        "compatible request uses parallel tools without a negotiated implemented mapping",
    )?;
    require_capability_mapping(
        profile,
        request,
        matches!(request.options().output(), StructuredOutput::JsonSchema { strict: true, .. }),
        Capability::StrictStructuredOutput,
        RequestField::StrictStructuredOutput,
        "compatible request uses strict structured output without an implemented mapping",
    )?;
    require_capability_mapping(
        profile,
        request,
        blocks().any(|block| matches!(block, ContentBlock::Image(_))),
        Capability::ImageInput,
        RequestField::ImageInput,
        "compatible request uses image input without a negotiated implemented mapping",
    )?;
    require_capability_mapping(
        profile,
        request,
        blocks().any(|block| matches!(block, ContentBlock::Reasoning(_))),
        Capability::ReasoningReplay,
        RequestField::ReasoningReplay,
        "compatible request uses reasoning replay without a reviewed hosted mapping",
    )?;

    let generation = request.options().generation();
    require_capability_mapping(
        profile,
        request,
        generation.temperature_millionths().is_some(),
        Capability::SamplingControls,
        RequestField::Temperature,
        "compatible request uses temperature without a negotiated implemented mapping",
    )?;
    require_capability_mapping(
        profile,
        request,
        generation.top_p_millionths().is_some(),
        Capability::SamplingControls,
        RequestField::TopP,
        "compatible request uses top-p without a negotiated implemented mapping",
    )?;
    require_capability_mapping(
        profile,
        request,
        generation.seed().is_some(),
        Capability::SamplingControls,
        RequestField::Seed,
        "compatible request uses seed without a negotiated implemented mapping",
    )?;
    require_capability_mapping(
        profile,
        request,
        !generation.stop_sequences().is_empty(),
        Capability::SamplingControls,
        RequestField::StopSequences,
        "compatible request uses stop sequences without a negotiated implemented mapping",
    )?;
    require_capability_mapping(
        profile,
        request,
        request.negotiated().includes(Capability::UsageDetail),
        Capability::UsageDetail,
        RequestField::Usage,
        "compatible request selects usage detail without an implemented mapping",
    )?;
    require_capability_mapping(
        profile,
        request,
        request.options().persistence().store(),
        Capability::StoredState,
        RequestField::StoredState,
        "compatible request selects provider storage without an implemented mapping",
    )?;
    if request.options().continuation().is_some()
        && !profile.contract().supports(RequestField::Continuation)
    {
        return Err(error::invalid(
            "compatible request continuation has no reviewed provider-side mapping",
        ));
    }
    Ok(())
}

fn require_capability_mapping(
    profile: &CompatibleProfile,
    request: &ModelRequest,
    used: bool,
    capability: Capability,
    field: RequestField,
    detail: &'static str,
) -> Result<(), ProviderCoreError> {
    if used
        && (!request.negotiated().includes(capability) || !profile.contract().supports(field))
    {
        return Err(error::invalid(detail));
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
