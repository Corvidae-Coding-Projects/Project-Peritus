//! Layered context accounting and digest-bound transcript compaction for developer roles.

use core::fmt::Write as _;

use peritus_context::TokenBudget;
use peritus_model_protocol::{
    ContentBlock, Message, ProtocolLimits, ProviderProfile, Role, ToolDefinition,
};
use peritus_types::Sha256Digest;

use super::context_encoding::{
    POLICY_REVISION, encode_source, estimate_bytes, estimated_request_tokens,
};
use super::{DeveloperContextCompaction, DeveloperLoopError};

const RETAIN_RECENT_MESSAGES: usize = 8;
const COMPACTION_TRIGGER_PERCENT: u64 = 85;
const RESULT_PREVIEW_CHARS: usize = 512;
const TEXT_PREVIEW_CHARS: usize = 256;
pub(super) const COMPACTION_OPEN: &str = "<peritus-compaction ";

/// One complete old transcript prefix eligible for semantic or deterministic compaction.
pub(super) struct CompactionCandidate {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) source: Vec<Message>,
    pub(super) source_digest: Sha256Digest,
    pub(super) replaced_tokens: u64,
}

impl CompactionCandidate {
    pub(super) fn source(&self) -> &[Message] {
        &self.source
    }
}

/// Compacts complete old transcript units until the request fits its input limit.
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
    let (usable_input, trigger) = context_thresholds(profile, reserved_output_tokens)?;
    let mut records = Vec::new();
    while estimated_request_tokens(messages, tools) > trigger {
        let Some(record) = compact_once(messages, limits, RETAIN_RECENT_MESSAGES)? else {
            break;
        };
        records.push(record);
    }
    while estimated_request_tokens(messages, tools) > usable_input {
        let Some(record) = compact_once(messages, limits, 0)? else {
            break;
        };
        records.push(record);
    }
    let estimated = estimated_request_tokens(messages, tools);
    if estimated > usable_input {
        return Err(DeveloperLoopError::Context(format!(
            "estimated input tokens {estimated} exceed provider limit {usable_input} after deterministic compaction"
        )));
    }
    Ok(records)
}

fn compact_once(
    messages: &mut Vec<Message>,
    limits: ProtocolLimits,
    retain_recent_messages: usize,
) -> Result<Option<DeveloperContextCompaction>, DeveloperLoopError> {
    let Some(candidate) = compaction_candidate(messages, retain_recent_messages)? else {
        return Ok(None);
    };
    let replacement = render_compaction(&candidate.source, candidate.source_digest);
    let replacement_tokens = estimate_bytes(replacement.len());
    if replacement_tokens >= candidate.replaced_tokens {
        return Ok(None);
    }
    let replacement_digest = peritus_codec::sha256(replacement.as_bytes());
    let source_messages = u16::try_from(candidate.source.len()).map_err(|_| {
        DeveloperLoopError::Context("compaction source count exceeded u16".to_owned())
    })?;
    let record = DeveloperContextCompaction::new(
        [peritus_codec::sha256(POLICY_REVISION), candidate.source_digest, replacement_digest],
        source_messages,
        candidate.replaced_tokens,
        replacement_tokens,
    );
    let message = Message::new(
        Role::User,
        vec![ContentBlock::Text(peritus_model_protocol::BoundedText::new(replacement, limits)?)],
        limits,
    )?;
    messages.splice(candidate.start..candidate.end, [message]);
    Ok(Some(record))
}

pub(super) fn context_thresholds(
    profile: &ProviderProfile,
    reserved_output_tokens: u64,
) -> Result<(u64, u64), DeveloperLoopError> {
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
    let usable = budget.usable_input();
    let trigger = (usable.saturating_mul(COMPACTION_TRIGGER_PERCENT) / 100).max(1);
    Ok((usable, trigger))
}

pub(super) fn compaction_candidate(
    messages: &[Message],
    retain_recent_messages: usize,
) -> Result<Option<CompactionCandidate>, DeveloperLoopError> {
    let start = 2;
    let cutoff = messages.len().saturating_sub(retain_recent_messages);
    if cutoff <= start {
        return Ok(None);
    }
    let mut cursor = start;
    while cursor < cutoff {
        let Some(end) = complete_unit_end(messages, cursor, cutoff) else {
            break;
        };
        cursor = end;
    }
    if cursor == start {
        return Ok(None);
    }
    let source = messages[start..cursor].to_vec();
    let source_bytes = encode_source(&source)?;
    Ok(Some(CompactionCandidate {
        start,
        end: cursor,
        source,
        source_digest: peritus_codec::sha256(&source_bytes),
        replaced_tokens: estimate_bytes(source_bytes.len()),
    }))
}

fn complete_unit_end(messages: &[Message], start: usize, cutoff: usize) -> Option<usize> {
    let message = messages.get(start)?;
    match message.role() {
        Role::User
            if message.content().iter().all(|block| matches!(block, ContentBlock::Text(_))) =>
        {
            Some(start + 1)
        }
        Role::Assistant => {
            let has_calls =
                message.content().iter().any(|block| matches!(block, ContentBlock::ToolCall(_)));
            if has_calls {
                tool_exchange_end(messages, start, cutoff)
            } else if message.content().iter().all(|block| matches!(block, ContentBlock::Text(_))) {
                Some(start + 1)
            } else {
                None
            }
        }
        Role::System | Role::Developer | Role::Tool | Role::User => None,
    }
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
                let role = match message.role() {
                    Role::User => "user_text",
                    Role::Assistant => "assistant_text",
                    Role::System | Role::Developer | Role::Tool => "other_text",
                };
                let _ = write!(output, "{role} sha256={digest} preview={preview}");
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
    ["prior_lineage ", "user_text ", "assistant_text ", "tool_call ", "tool_result "]
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

pub(super) fn digest_hex(digest: Sha256Digest) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest.as_bytes() {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
