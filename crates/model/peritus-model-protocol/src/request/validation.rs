//! Aggregate request bounds and capability validation.

use std::collections::BTreeSet;

use super::RequestOptions;
use crate::{
    Capability, ContentBlock, Message, NegotiatedCapabilities, ParallelToolPolicy, ProtocolError,
    ProtocolErrorKind, ProtocolLimits, ResumeKind, StructuredOutput, ToolChoice, ToolDefinition,
};

pub(super) fn request(
    negotiated: NegotiatedCapabilities,
    messages: &[Message],
    tools: &[ToolDefinition],
    tool_choice: &ToolChoice,
    parallel: ParallelToolPolicy,
    options: &RequestOptions,
    resume: ResumeKind,
    limits: ProtocolLimits,
) -> Result<(), ProtocolError> {
    validate_counts(messages, tools, options, limits)?;
    crate::tool::validate_choice(tool_choice, tools)?;
    if tools.len() > usize::try_from(negotiated.limits().max_tools()).unwrap_or(usize::MAX) {
        return Err(invalid("tools", "tool count exceeds the negotiated model limit"));
    }
    validate_capabilities(negotiated, messages, tools, parallel, options, resume)?;
    crate::tool::validate_parallel(
        parallel,
        negotiated.limits().max_parallel_tool_calls(),
        limits,
    )?;
    if options.generation().max_output_tokens() > negotiated.limits().max_output_tokens() {
        return Err(invalid("generation", "output token request exceeds negotiated limit"));
    }
    Ok(())
}

fn validate_counts(
    messages: &[Message],
    tools: &[ToolDefinition],
    options: &RequestOptions,
    limits: ProtocolLimits,
) -> Result<(), ProtocolError> {
    if messages.is_empty() || messages.len() > limits.max_messages() {
        return Err(invalid("messages", "message count is empty or exceeds its bound"));
    }
    let blocks = messages
        .iter()
        .try_fold(0_usize, |total, message| total.checked_add(message.content().len()));
    if blocks.is_none_or(|count| count > limits.max_content_blocks()) {
        return Err(invalid("messages", "aggregate content-block count exceeds its bound"));
    }
    let media = messages
        .iter()
        .flat_map(Message::content)
        .try_fold(0_usize, |total, block| total.checked_add(block.inline_media_bytes()));
    if media.is_none_or(|bytes| bytes > limits.max_total_media_bytes()) {
        return Err(invalid("messages", "aggregate inline-media bytes exceed their bound"));
    }
    if tools.len() > limits.max_tools() {
        return Err(invalid("tools", "tool count exceeds its request bound"));
    }
    let mut names = BTreeSet::new();
    if tools.iter().any(|tool| !names.insert(tool.name().as_str())) {
        return Err(invalid("tools", "tool names must be unique"));
    }
    Ok(())
}

fn validate_capabilities(
    negotiated: NegotiatedCapabilities,
    messages: &[Message],
    tools: &[ToolDefinition],
    parallel: ParallelToolPolicy,
    options: &RequestOptions,
    resume: ResumeKind,
) -> Result<(), ProtocolError> {
    for block in messages.iter().flat_map(Message::content) {
        let bytes = u64::try_from(block.inline_media_bytes())
            .map_err(|_| invalid("messages", "inline media byte count exceeds u64"))?;
        if bytes > negotiated.limits().max_inline_media_bytes() {
            return Err(invalid(
                "messages",
                "inline media payload exceeds the negotiated model limit",
            ));
        }
        let required = match block {
            ContentBlock::Image(_) => Some(Capability::ImageInput),
            ContentBlock::Audio(_) => Some(Capability::AudioInput),
            ContentBlock::Document(_) => Some(Capability::DocumentInput),
            ContentBlock::ToolCall(_) | ContentBlock::ToolResult(_) => Some(Capability::ToolCalls),
            ContentBlock::Reasoning(_) if negotiated.includes(Capability::ReasoningControls) => {
                None
            }
            ContentBlock::Reasoning(_) => Some(Capability::ReasoningReplay),
            ContentBlock::ProviderExtension(_) => Some(Capability::ProviderExtensions),
            ContentBlock::Text(_) | ContentBlock::Refusal(_) => None,
        };
        if required.is_some_and(|capability| !negotiated.includes(capability)) {
            return Err(invalid("messages", "content requires an unnegotiated capability"));
        }
    }
    require(!tools.is_empty(), Capability::ToolCalls, negotiated, "tools")?;
    require(
        matches!(parallel, ParallelToolPolicy::Allowed(_)),
        Capability::ParallelToolCalls,
        negotiated,
        "parallel_tools",
    )?;
    require(
        matches!(options.output(), StructuredOutput::JsonSchema { strict: true, .. }),
        Capability::StrictStructuredOutput,
        negotiated,
        "structured_output",
    )?;
    require(options.reasoning().enabled(), Capability::ReasoningControls, negotiated, "reasoning")?;
    require(
        options.reasoning().requests_summary(),
        Capability::ReasoningSummaries,
        negotiated,
        "reasoning_summary",
    )?;
    require(
        !matches!(options.cache(), super::CachePolicy::Disabled),
        Capability::PromptCaching,
        negotiated,
        "cache",
    )?;
    require(options.persistence().store(), Capability::StoredState, negotiated, "persistence")?;
    validate_continuation(negotiated, options, resume)?;
    require(
        !options.extensions().is_empty(),
        Capability::ProviderExtensions,
        negotiated,
        "extensions",
    )?;
    require(
        options.generation().uses_sampling(),
        Capability::SamplingControls,
        negotiated,
        "sampling",
    )
}

fn validate_continuation(
    negotiated: NegotiatedCapabilities,
    options: &RequestOptions,
    resume: ResumeKind,
) -> Result<(), ProtocolError> {
    let Some(continuation) = options.continuation() else { return Ok(()) };
    match (resume, continuation.is_exact()) {
        (ResumeKind::Unsupported, _) => Err(unsupported(
            "continuation",
            "selected provider profile does not support response continuation",
        )),
        (ResumeKind::SemanticContinuation, false) => Ok(()),
        (ResumeKind::SemanticContinuation, true) => Err(unsupported(
            "continuation",
            "selected provider profile supports semantic continuation, not exact cursor resumption",
        )),
        (ResumeKind::ExactCursor, false) => Err(unsupported(
            "continuation",
            "selected provider profile requires an exact cursor for response resumption",
        )),
        (ResumeKind::ExactCursor, true)
            if !negotiated.includes(Capability::ResumableResponse) =>
        {
            Err(unsupported(
                "continuation",
                "exact cursor continuation requires the negotiated resumable_response capability",
            ))
        }
        (ResumeKind::ExactCursor, true) => Ok(()),
    }
}

fn require(
    used: bool,
    capability: Capability,
    negotiated: NegotiatedCapabilities,
    path: &'static str,
) -> Result<(), ProtocolError> {
    if used && !negotiated.includes(capability) {
        return Err(invalid(path, "request uses an unnegotiated capability"));
    }
    Ok(())
}

pub(super) fn invalid(path: &'static str, detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidRequest, path, detail)
}

fn unsupported(path: &'static str, detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::UnsupportedCapability, path, detail)
}
