//! Fit a known adapter envelope using retrievable prefixes of already durable observations.

use super::super::{error, memory::LocalMemory, record::ArchiveKind, tools::source_handle};
use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::{
    CanonicalJson, ContentBlock, JsonBounds, Message, ProtocolLimits, ToolDefinition, ToolResult,
};
use peritus_provider_core::ModelProvider;

pub(super) fn fit(
    memory: &mut LocalMemory,
    messages: &mut [Message],
    tools: &[ToolDefinition],
    provider: &dyn ModelProvider,
) -> Result<bool, DeveloperLoopError> {
    let mut collapsed = false;
    if provider
        .context_input_fits(messages, tools)
        .map_err(|_| error("encode selected provider context projection"))?
    {
        return Ok(collapsed);
    }
    // Derived records are retrievable data, not governing instructions. A large working
    // model must not become an unshrinkable required closure at the adapter boundary.
    for message in messages.iter_mut() {
        if super::working::is_view(message) {
            collapsed = true;
            *message = super::text_message(
                peritus_model_protocol::Role::User,
                super::working::notice(memory, &memory.state)?,
            )?;
        }
    }
    if provider
        .context_input_fits(messages, tools)
        .map_err(|_| error("encode paged working view"))?
    {
        return Ok(collapsed);
    }
    for index in 0..messages.len() {
        for block in 0..messages[index].content().len() {
            if fit_result(memory, messages, tools, provider, index, block)? {
                return Ok(collapsed);
            }
        }
    }
    Err(error(
        "current required instructions exceed the selected provider's submitted-turn capacity; use a provider that accepts larger input",
    ))
}

fn fit_result(
    memory: &mut LocalMemory,
    messages: &mut [Message],
    tools: &[ToolDefinition],
    provider: &dyn ModelProvider,
    index: usize,
    block: usize,
) -> Result<bool, DeveloperLoopError> {
    let ContentBlock::ToolResult(result) = &messages[index].content()[block] else {
        return Ok(false);
    };
    if result.output().canonical_bytes().len() <= 4_096 {
        return Ok(false);
    }
    let original = result.clone();
    let Some(source) = memory.sources.iter().rev().find(|source| {
        source.kind == ArchiveKind::ToolOutput
            && source
                .call
                .as_ref()
                .is_some_and(|call| call.id == original.call_id().expose_for_wire())
    }) else {
        return Err(error("oversized tool view has no durable exact source"));
    };
    let source_id = source.sequence;
    let exact_bytes = memory.artifact(source_id)?;
    let exact = std::str::from_utf8(&exact_bytes)
        .map_err(|_| error("archived JSON observation is not UTF-8"))?;
    let mut source_value: serde_json::Value =
        serde_json::from_str(exact).map_err(|_| error("decode archived transport source"))?;
    preserve_temporal_fields(&original, &mut source_value);
    let handle = source_handle(memory, source_id);
    let mut content = messages[index].content().to_vec();
    let role = messages[index].role();
    let mut install = |end: usize, messages: &mut [Message]| -> Result<(), DeveloperLoopError> {
        let end = exact.floor_char_boundary(end);
        let value = super::super::read_reuse::provider_page(memory, source_id, &source_value, end)
            .unwrap_or_else(|| fallback_view(&handle, exact, end));
        super::super::read_reuse::register_rendered(memory, &value.to_string(), source_id, &value);
        let output = CanonicalJson::parse(
            &value.to_string(),
            JsonBounds::value(ProtocolLimits::PRODUCTION),
        )?;
        content[block] = ContentBlock::ToolResult(ToolResult::new(
            original.call_id().clone(),
            output,
            original.is_error(),
        ));
        messages[index] = Message::new(role, content.clone(), ProtocolLimits::PRODUCTION)?;
        Ok(())
    };
    install(0, messages)?;
    if !provider
        .context_input_fits(messages, tools)
        .map_err(|_| error("encode selected provider context projection"))?
    {
        return Ok(false);
    }
    let mut low = 0;
    let mut high = exact.len();
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        install(middle, messages)?;
        if provider
            .context_input_fits(messages, tools)
            .map_err(|_| error("encode selected provider context projection"))?
        {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    install(low, messages)?;
    Ok(true)
}

fn preserve_temporal_fields(original: &ToolResult, value: &mut serde_json::Value) {
    let Ok(projected) =
        serde_json::from_slice::<serde_json::Value>(original.output().canonical_bytes())
    else {
        return;
    };
    if projected.get("observation_time").is_none() {
        return;
    }
    for key in ["cached_observation_stale", "observation_time", "reason", "original_observation"] {
        if let Some(field) = projected.get(key) {
            value[key] = field.clone();
        }
    }
}

fn fallback_view(handle: &str, exact: &str, end: usize) -> serde_json::Value {
    serde_json::json!({
        "peritus_provider_view": {
            "truncated": true,
            "reason": "The full observation exceeds this provider's submitted-turn character capacity. It is durably retained, and this is a contiguous prefix of its JSON bytes, not a complete file-read result. Retrieve the remainder with context_read using the exact source handle and next_offset; do not repeat settled effects.",
            "source": handle,
            "original_bytes": exact.len(),
            "visible_bytes": end,
            "next_offset": end,
            "prefix": &exact[..end],
        }
    })
}
