//! Deterministic context accounting and transcript compaction for developer roles.

use core::fmt::Write as _;

use peritus_context::TokenBudget;
use peritus_model_protocol::{
    ContentBlock, Message, ProtocolLimits, ProviderProfile, Role, ToolDefinition,
};
use peritus_types::Sha256Digest;

use super::{DeveloperContextCompaction, DeveloperLoopError};

const POLICY_REVISION: &[u8] = b"peritus-developer-compaction-v2";
const RETAIN_RECENT_MESSAGES: usize = 8;
const TOKEN_ESTIMATE_BYTES: u64 = 3;
const COMPACTION_TRIGGER_PERCENT: u64 = 85;
const RESULT_PREVIEW_CHARS: usize = 512;
const TEXT_PREVIEW_CHARS: usize = 256;
const COMPACTION_OPEN: &str = "<peritus-compaction ";

/// Compacts complete tool exchanges until the request fits its input limit.
///
/// The normal pass retains recent exchanges. A second pass compacts a recent complete exchange
/// only when retaining it would make the next provider request impossible.
pub(super) fn prepare_messages(
    messages: &mut Vec<Message>,
    tools: &[ToolDefinition],
    profile: &ProviderProfile,
    reserved_output_tokens: u64,
    limits: ProtocolLimits,
) -> Result<Vec<DeveloperContextCompaction>, DeveloperLoopError> {
    let input = profile.limits().max_input_tokens();
    let protocol = input.div_ceil(20).clamp(512, 4_096);
    let context_window = input
        .checked_add(reserved_output_tokens)
        .and_then(|value| value.checked_add(protocol))
        .ok_or_else(|| {
            DeveloperLoopError::Context("context token capacity overflowed".to_owned())
        })?;
    let budget =
        TokenBudget::new(context_window, reserved_output_tokens, protocol).map_err(|error| {
            DeveloperLoopError::Context(format!("C6 token budget rejected: {:?}", error.kind()))
        })?;
    let trigger = (budget.usable_input().saturating_mul(COMPACTION_TRIGGER_PERCENT) / 100).max(1);
    let mut records = Vec::new();
    while estimated_request_tokens(messages, tools) > trigger {
        let Some(record) = compact_once(messages, limits, RETAIN_RECENT_MESSAGES)? else {
            break;
        };
        records.push(record);
    }
    while estimated_request_tokens(messages, tools) > budget.usable_input() {
        let Some(record) = compact_once(messages, limits, 0)? else {
            break;
        };
        records.push(record);
    }
    let estimated = estimated_request_tokens(messages, tools);
    if estimated > budget.usable_input() {
        return Err(DeveloperLoopError::Context(format!(
            "estimated input tokens {estimated} exceed provider limit {} after deterministic compaction",
            budget.usable_input()
        )));
    }
    Ok(records)
}

fn compact_once(
    messages: &mut Vec<Message>,
    limits: ProtocolLimits,
    retain_recent_messages: usize,
) -> Result<Option<DeveloperContextCompaction>, DeveloperLoopError> {
    let cutoff = messages.len().saturating_sub(retain_recent_messages);
    if cutoff <= 2 {
        return Ok(None);
    }
    let mut start = 2;
    while start < cutoff {
        let mut cursor = start;
        if is_compaction(&messages[cursor]) {
            cursor += 1;
        }
        let mut exchanges = 0_u16;
        while cursor < cutoff {
            let Some(end) = tool_exchange_end(messages, cursor, cutoff) else {
                break;
            };
            exchanges = exchanges.checked_add(1).ok_or_else(|| {
                DeveloperLoopError::Context("tool exchange count overflowed".to_owned())
            })?;
            cursor = end;
        }
        if exchanges > 0 {
            let source = &messages[start..cursor];
            let source_bytes = encode_source(source)?;
            let source_digest = peritus_codec::sha256(&source_bytes);
            let replacement = render_compaction(source, source_digest);
            let replaced_tokens = estimate_bytes(source_bytes.len());
            let replacement_tokens = estimate_bytes(replacement.len());
            if replacement_tokens < replaced_tokens {
                let replacement_digest = peritus_codec::sha256(replacement.as_bytes());
                let source_messages = u16::try_from(source.len()).map_err(|_| {
                    DeveloperLoopError::Context("compaction source count exceeded u16".to_owned())
                })?;
                let record = DeveloperContextCompaction::new(
                    [peritus_codec::sha256(POLICY_REVISION), source_digest, replacement_digest],
                    source_messages,
                    replaced_tokens,
                    replacement_tokens,
                );
                let message = Message::new(
                    Role::User,
                    vec![ContentBlock::Text(peritus_model_protocol::BoundedText::new(
                        replacement,
                        limits,
                    )?)],
                    limits,
                )?;
                messages.splice(start..cursor, [message]);
                return Ok(Some(record));
            }
        }
        start = cursor.max(start + 1);
    }
    Ok(None)
}

fn tool_exchange_end(messages: &[Message], start: usize, cutoff: usize) -> Option<usize> {
    let assistant = messages.get(start)?;
    if assistant.role() != Role::Assistant
        || assistant
            .content()
            .iter()
            .any(|block| !matches!(block, ContentBlock::Text(_) | ContentBlock::ToolCall(_)))
    {
        return None;
    }
    let call_ids = assistant
        .content()
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolCall(call) => Some(call.id().expose_for_wire()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if call_ids.is_empty() {
        return None;
    }
    let mut observed = vec![false; call_ids.len()];
    let mut cursor = start + 1;
    while cursor < cutoff && observed.iter().any(|value| !value) {
        let message = &messages[cursor];
        if message.role() != Role::Tool || message.content().is_empty() {
            return None;
        }
        for block in message.content() {
            let ContentBlock::ToolResult(result) = block else {
                return None;
            };
            let position =
                call_ids.iter().position(|value| *value == result.call_id().expose_for_wire())?;
            if observed[position] {
                return None;
            }
            observed[position] = true;
        }
        cursor += 1;
    }
    observed.iter().all(|value| *value).then_some(cursor)
}

fn is_compaction(message: &Message) -> bool {
    message.role() == Role::User
        && matches!(
            message.content(),
            [ContentBlock::Text(text)] if text.expose_for_wire().starts_with(COMPACTION_OPEN)
        )
}

fn render_compaction(messages: &[Message], source_digest: Sha256Digest) -> String {
    let policy = digest_hex(peritus_codec::sha256(POLICY_REVISION));
    let source = digest_hex(source_digest);
    let mut output = format!(
        "{COMPACTION_OPEN}version=\"1\" policy_sha256=\"{policy}\" source_sha256=\"{source}\">\n"
    );
    output.push_str(
        "Non-authoritative deterministic record of earlier completed tool exchanges. Exact outputs remain bound by SHA-256; use fresh tools when current state matters.\n",
    );
    for message in messages {
        append_message_summary(&mut output, message);
    }
    output.push_str("</peritus-compaction>");
    output
}

fn append_message_summary(output: &mut String, message: &Message) {
    if is_compaction(message) {
        let ContentBlock::Text(text) = &message.content()[0] else {
            return;
        };
        let digest = digest_hex(peritus_codec::sha256(text.expose_for_wire().as_bytes()));
        let _ = write!(output, "prior_lineage replacement_sha256={digest}");
        output.push('\n');
        for line in text.expose_for_wire().lines().filter(|line| retained_summary_line(line)) {
            output.push_str(line);
            output.push('\n');
        }
        return;
    }
    for block in message.content() {
        match block {
            ContentBlock::Text(text) => {
                let digest = digest_hex(peritus_codec::sha256(text.expose_for_wire().as_bytes()));
                let preview = quoted_preview(text.expose_for_wire(), TEXT_PREVIEW_CHARS);
                let _ = write!(output, "assistant_text sha256={digest} preview={preview}");
                output.push('\n');
            }
            ContentBlock::ToolCall(call) => {
                let arguments = call.arguments().to_wire_string();
                let digest = digest_hex(peritus_codec::sha256(arguments.as_bytes()));
                let preview = quoted_preview(&arguments, RESULT_PREVIEW_CHARS);
                let _ = write!(
                    output,
                    "tool_call id={} name={} arguments_sha256={digest} arguments={preview}",
                    call.id().expose_for_wire(),
                    call.name().as_str()
                );
                output.push('\n');
            }
            ContentBlock::ToolResult(result) => {
                let value = result.output().to_wire_string();
                let digest = digest_hex(peritus_codec::sha256(value.as_bytes()));
                let preview = quoted_preview(&value, RESULT_PREVIEW_CHARS);
                let _ = write!(
                    output,
                    "tool_result id={} error={} output_sha256={digest} output={preview}",
                    result.call_id().expose_for_wire(),
                    result.is_error()
                );
                output.push('\n');
            }
            ContentBlock::Image(_)
            | ContentBlock::Audio(_)
            | ContentBlock::Document(_)
            | ContentBlock::Refusal(_)
            | ContentBlock::Reasoning(_)
            | ContentBlock::ProviderExtension(_) => {}
        }
    }
}

fn retained_summary_line(line: &str) -> bool {
    ["prior_lineage ", "assistant_text ", "tool_call ", "tool_result "]
        .iter()
        .any(|prefix| line.starts_with(prefix))
}

fn quoted_preview(value: &str, limit: usize) -> String {
    let mut preview = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        preview.push('…');
    }
    format!("{preview:?}")
}

fn estimated_request_tokens(messages: &[Message], tools: &[ToolDefinition]) -> u64 {
    let message_bytes = messages.iter().map(message_bytes).fold(0_u64, u64::saturating_add);
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

fn message_bytes(message: &Message) -> u64 {
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

fn encode_source(messages: &[Message]) -> Result<Vec<u8>, DeveloperLoopError> {
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

const fn estimate_bytes(bytes: usize) -> u64 {
    estimate_u64(bytes as u64)
}

const fn estimate_u64(bytes: u64) -> u64 {
    let estimate = bytes.div_ceil(TOKEN_ESTIMATE_BYTES);
    if estimate == 0 { 1 } else { estimate }
}

fn digest_hex(digest: Sha256Digest) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest.as_bytes() {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
