//! Deterministic token estimates and canonical transcript-source encoding.

use peritus_model_protocol::{ContentBlock, Message, Role, ToolDefinition};

use super::DeveloperLoopError;

pub(super) const POLICY_REVISION: &[u8] = b"peritus-developer-compaction-v3";
const TOKEN_ESTIMATE_BYTES: u64 = 3;

pub(super) fn estimated_request_tokens(messages: &[Message], tools: &[ToolDefinition]) -> u64 {
    let message_bytes = messages.iter().map(encoded_message_size).fold(0_u64, u64::saturating_add);
    let tool_bytes = tools.iter().fold(0_u64, |total, tool| {
        total
            .saturating_add(tool.name().as_str().len() as u64)
            .saturating_add(
                tool.description().map_or(0, peritus_model_protocol::BoundedText::len) as u64
            )
            .saturating_add(tool.parameters().canonical_bytes().len() as u64)
            .saturating_add(32)
    });
    estimate_u64(message_bytes.saturating_add(tool_bytes).saturating_add(512))
}

fn encoded_message_size(message: &Message) -> u64 {
    message.content().iter().fold(16_u64, |total, block| {
        total.saturating_add(match block {
            ContentBlock::Text(value) | ContentBlock::Refusal(value) => value.len() as u64,
            ContentBlock::Image(media)
            | ContentBlock::Audio(media)
            | ContentBlock::Document(media) => {
                media.inline_bytes_for_wire().map_or(256, |bytes| bytes.len() as u64)
            }
            ContentBlock::ToolCall(call) => {
                call.arguments()
                    .canonical_bytes()
                    .len()
                    .saturating_add(call.id().expose_for_wire().len())
                    .saturating_add(call.name().as_str().len()) as u64
            }
            ContentBlock::ToolResult(result) => result
                .output()
                .canonical_bytes()
                .len()
                .saturating_add(result.call_id().expose_for_wire().len())
                as u64,
            ContentBlock::Reasoning(value) => value
                .summary()
                .map_or(0, peritus_model_protocol::BoundedText::len)
                .saturating_add(value.opaque_for_wire().len())
                as u64,
            ContentBlock::ProviderExtension(value) => value.value().canonical_bytes().len() as u64,
        })
    })
}

pub(super) fn encode_source(messages: &[Message]) -> Result<Vec<u8>, DeveloperLoopError> {
    let mut output = Vec::new();
    output.extend_from_slice(POLICY_REVISION);
    append_u64(&mut output, messages.len() as u64);
    for message in messages {
        output.push(match message.role() {
            Role::System => 1,
            Role::Developer => 2,
            Role::User => 3,
            Role::Assistant => 4,
            Role::Tool => 5,
        });
        append_u64(&mut output, message.content().len() as u64);
        for block in message.content() {
            match block {
                ContentBlock::Text(value) => {
                    append_tagged(&mut output, 1, value.expose_for_wire().as_bytes());
                }
                ContentBlock::ToolCall(call) => {
                    append_tagged(&mut output, 2, call.id().expose_for_wire().as_bytes());
                    append_tagged(&mut output, 3, call.name().as_str().as_bytes());
                    append_tagged(&mut output, 4, call.arguments().canonical_bytes());
                }
                ContentBlock::ToolResult(result) => {
                    append_tagged(&mut output, 5, result.call_id().expose_for_wire().as_bytes());
                    output.push(u8::from(result.is_error()));
                    append_tagged(&mut output, 6, result.output().canonical_bytes());
                }
                ContentBlock::Image(_)
                | ContentBlock::Audio(_)
                | ContentBlock::Document(_)
                | ContentBlock::Refusal(_)
                | ContentBlock::Reasoning(_)
                | ContentBlock::ProviderExtension(_) => {
                    return Err(DeveloperLoopError::Context(
                        "compaction source contained an unsupported content block".to_owned(),
                    ));
                }
            }
        }
    }
    Ok(output)
}

fn append_tagged(output: &mut Vec<u8>, tag: u8, bytes: &[u8]) {
    output.push(tag);
    append_u64(output, bytes.len() as u64);
    output.extend_from_slice(bytes);
}

fn append_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(super) const fn estimate_bytes(bytes: usize) -> u64 {
    estimate_u64(bytes as u64)
}

const fn estimate_u64(bytes: u64) -> u64 {
    let estimate = bytes.div_ceil(TOKEN_ESTIMATE_BYTES);
    if estimate == 0 { 1 } else { estimate }
}
