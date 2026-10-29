//! Original canonical tool bytes, with explicit origins and exact continuation offsets.

use super::index::Evidence;
use crate::local_context::{error, memory::LocalMemory, tools::source_handle};
use peritus_agent::{DeveloperContextEstimate, DeveloperLoopError};
use peritus_model_protocol::{ContentBlock, Message, Role};

const PREFIX: &str = "DECISION TOOL EVIDENCE V1";

pub(super) fn sequence(message: &Message) -> u64 {
    message
        .content()
        .iter()
        .find_map(|block| {
            let ContentBlock::Text(text) = block else { return None };
            text.expose_for_wire().lines().nth(1)?.strip_prefix("sequence=")?.parse().ok()
        })
        .unwrap_or(u64::MAX)
}

pub(super) fn fit(
    memory: &mut LocalMemory,
    evidence: &Evidence,
    required: DeveloperContextEstimate,
    maximum: u64,
    bindings: &super::bindings::Bindings,
) -> Result<Option<(Message, bool)>, DeveloperLoopError> {
    let bytes = memory.store.read(evidence.artifact)?;
    let exact = std::str::from_utf8(&bytes)
        .map_err(|_| error("decision tool source is not canonical UTF-8"))?;
    let arguments = arguments(memory, evidence)?;
    let make = |end: usize| {
        let end = exact.floor_char_boundary(end);
        super::super::text_message(
            Role::User,
            format!(
                "{PREFIX}\nsequence={}\nsource={} tool={} is_error={} source_bytes={} retrieval={} original_arguments={} current_content={}\noffset=0 visible_bytes={end} next_offset={}\n{}\n{}",
                evidence.source,
                source_handle(memory, evidence.source),
                evidence.tool,
                evidence.is_error,
                evidence.artifact.bytes,
                if evidence.host_step { "inspection_result_read" } else { "context_read" },
                arguments,
                content_matches(evidence, exact, bindings),
                if end < exact.len() { end.to_string() } else { "null".to_owned() },
                if end < exact.len() {
                    "RAW JSON PREFIX ONLY; not a complete file page. Retrieve remaining original bytes with the named retrieval tool."
                } else {
                    "EXACT ORIGINAL JSON BYTES:"
                },
                &exact[..end],
            ),
        )
    };
    let check = |card: &Message| {
        required.with(DeveloperContextEstimate::messages(std::slice::from_ref(card))).tokens()
            <= maximum
    };
    let full = make(exact.len())?;
    let (card, clipped) = if check(&full) {
        (full, false)
    } else {
        if !check(&make(0)?) {
            return Ok(None);
        }
        let (mut low, mut high) = (0, exact.len());
        while low < high {
            let middle = low + (high - low).div_ceil(2);
            if check(&make(middle)?) {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        (make(low)?, true)
    };
    if !clipped && !evidence.is_error && evidence.tool == "workspace_read" {
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| error("decode decision read source"))?;
        if super::super::evidence::read_is_current(memory, &value) {
            let ContentBlock::Text(body) = &card.content()[0] else {
                return Err(error("decision card framing"));
            };
            crate::local_context::read_reuse::register_rendered(
                memory,
                body.expose_for_wire(),
                evidence.source,
                &value,
            );
        }
    }
    Ok(Some((card, clipped)))
}

fn content_matches(
    evidence: &Evidence,
    exact: &str,
    bindings: &super::bindings::Bindings,
) -> &'static str {
    let Some(current) = evidence.file.as_ref().and_then(|path| bindings.files.get(path)) else {
        return "unproven";
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(exact) else { return "unproven" };
    match (current["content_sha256"].as_str(), value["source_sha256"].as_str()) {
        (Some(current), Some(original)) if current == original => "same_content_only",
        (Some(_), Some(_)) => "changed",
        _ => "unavailable",
    }
}

fn arguments(memory: &LocalMemory, evidence: &Evidence) -> Result<String, DeveloperLoopError> {
    if evidence.host_step {
        let crate::local_context::operation_plan::PlanRecord::Step { arguments, .. } =
            crate::local_context::operation_plan::load_record(memory, evidence.source)?
        else {
            return Err(error("decision host-step identity changed"));
        };
        return Ok(arguments.to_string());
    }
    let Some((id, block)) = evidence.arguments_message else {
        return Ok(format!(
            "unavailable; original digest={}",
            crate::local_context::tools::hex(&evidence.arguments)
        ));
    };
    let message = memory.message(id)?;
    let Some(ContentBlock::ToolCall(call)) = message.content().get(block) else {
        return Err(error("decision original tool proposal unavailable"));
    };
    if call.name().as_str() != evidence.tool
        || peritus_codec::sha256(call.arguments().canonical_bytes()).into_bytes()
            != evidence.arguments
    {
        return Err(error("decision source arguments do not match the original proposal"));
    }
    Ok(call.arguments().to_wire_string())
}
