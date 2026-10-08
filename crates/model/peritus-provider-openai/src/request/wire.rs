//! Exact bounded Responses JSON encoding without intermediate whole-value projection.

use std::io::{self, Write as _};

use base64::Engine as _;
use peritus_model_protocol::{
    CachePolicy, Capability, CompletedToolCall, ContentBlock, MediaInput, MediaReferenceKind,
    Message, ModelRequest, ParallelToolPolicy, ReasoningEffort, ReasoningPolicy, ReasoningReplay,
    Role, StructuredOutput, SummaryPolicy, ToolChoice, ToolDefinition, ToolResult,
};
use peritus_provider_core::ProviderCoreError;
use serde::Serialize;

use crate::error;

pub(super) fn encode(
    request: &ModelRequest,
    maximum: usize,
) -> Result<Vec<u8>, ProviderCoreError> {
    let mut counter = CountingSink::new(maximum);
    if write_request(&mut counter, request).is_err() {
        return Err(wire_error(counter.exceeded));
    }
    let exact = counter.length;
    let mut body = BodySink::new(exact);
    if write_request(&mut body, request).is_err() || body.bytes.len() != exact {
        return Err(wire_error(body.exceeded));
    }
    Ok(body.bytes)
}

fn write_request<S: WireSink>(sink: &mut S, request: &ModelRequest) -> io::Result<()> {
    sink.write_all(b"{\"model\":")?;
    write_json(sink, request.model().as_str())?;
    sink.write_all(b",\"input\":")?;
    write_input(sink, request)?;
    sink.write_all(b",\"stream\":true,\"store\":")?;
    write_bool(sink, request.options().persistence().store())?;
    sink.write_all(b",\"background\":")?;
    write_bool(sink, request.options().persistence().background())?;
    sink.write_all(b",\"max_output_tokens\":")?;
    write_json(sink, &request.options().generation().max_output_tokens())?;
    if let Some(continuation) = request
        .options()
        .continuation()
        .filter(|continuation| continuation.sequence().is_none())
    {
        sink.write_all(b",\"previous_response_id\":")?;
        write_json(sink, continuation.response_id().expose_for_wire())?;
    }
    if includes_reasoning(request) {
        sink.write_all(b",\"include\":[\"reasoning.encrypted_content\"]")?;
    }
    if !request.tools().is_empty() {
        sink.write_all(b",\"tools\":")?;
        write_tools(sink, request.tools())?;
    }
    if !request.tools().is_empty() || !matches!(request.tool_choice(), ToolChoice::Auto) {
        sink.write_all(b",\"tool_choice\":")?;
        write_tool_choice(sink, request.tool_choice())?;
    }
    sink.write_all(b",\"parallel_tool_calls\":")?;
    write_bool(
        sink,
        matches!(request.parallel_tool_policy(), ParallelToolPolicy::Allowed(_)),
    )?;
    sink.write_all(b",\"text\":")?;
    write_text_options(sink, request.options().output())?;
    if !matches!(request.options().reasoning(), ReasoningPolicy::Disabled) {
        sink.write_all(b",\"reasoning\":")?;
        write_reasoning_options(sink, request.options().reasoning())?;
    }
    if let Some(value) = request.options().generation().temperature_millionths() {
        sink.write_all(b",\"temperature\":")?;
        write_json(sink, &(f64::from(value) / 1_000_000.0))?;
    }
    if let Some(value) = request.options().generation().top_p_millionths() {
        sink.write_all(b",\"top_p\":")?;
        write_json(sink, &(f64::from(value) / 1_000_000.0))?;
    }
    if let CachePolicy::Explicit(key) = request.options().cache() {
        sink.write_all(b",\"prompt_cache_key\":")?;
        write_json(sink, key.expose_for_wire())?;
    }
    if matches!(request.options().cache(), CachePolicy::Ephemeral { .. }) {
        sink.write_all(b",\"prompt_cache_options\":{\"ttl\":\"30m\"}")?;
    }
    sink.write_all(b"}")
}

fn write_input<S: WireSink>(sink: &mut S, request: &ModelRequest) -> io::Result<()> {
    sink.write_all(b"[")?;
    let mut first = true;
    for message in request.messages() {
        write_message(sink, message, &mut first)?;
    }
    sink.write_all(b"]")
}

fn write_message<S: WireSink>(
    sink: &mut S,
    message: &Message,
    first: &mut bool,
) -> io::Result<()> {
    let content = message.content();
    let mut segment_start = 0;
    for (index, block) in content.iter().enumerate() {
        if is_message_part(block) {
            continue;
        }
        if segment_start < index {
            item_separator(sink, first)?;
            write_message_item(sink, message.role(), &content[segment_start..index])?;
        }
        item_separator(sink, first)?;
        write_standalone_item(sink, block)?;
        segment_start = index + 1;
    }
    if segment_start < content.len() {
        item_separator(sink, first)?;
        write_message_item(sink, message.role(), &content[segment_start..])?;
    }
    Ok(())
}

const fn is_message_part(block: &ContentBlock) -> bool {
    matches!(
        block,
        ContentBlock::Text(_)
            | ContentBlock::Image(_)
            | ContentBlock::Audio(_)
            | ContentBlock::Document(_)
            | ContentBlock::Refusal(_)
    )
}

fn item_separator<S: WireSink>(sink: &mut S, first: &mut bool) -> io::Result<()> {
    if *first {
        *first = false;
        Ok(())
    } else {
        sink.write_all(b",")
    }
}

fn write_message_item<S: WireSink>(
    sink: &mut S,
    role: Role,
    blocks: &[ContentBlock],
) -> io::Result<()> {
    sink.write_all(b"{\"content\":[")?;
    for (index, block) in blocks.iter().enumerate() {
        if index != 0 {
            sink.write_all(b",")?;
        }
        write_message_part(sink, role, block)?;
    }
    sink.write_all(b"],\"role\":")?;
    write_json(sink, role_name(role))?;
    sink.write_all(b",\"type\":\"message\"}")
}

fn write_message_part<S: WireSink>(
    sink: &mut S,
    role: Role,
    block: &ContentBlock,
) -> io::Result<()> {
    match block {
        ContentBlock::Text(text) => {
            sink.write_all(b"{\"text\":")?;
            write_json(sink, text.expose_for_wire())?;
            sink.write_all(if role == Role::Assistant {
                b",\"type\":\"output_text\"}"
            } else {
                b",\"type\":\"input_text\"}"
            })
        }
        ContentBlock::Image(media) => {
            write_media(sink, media, "input_image", "image_url", "file_id")
        }
        ContentBlock::Document(media) => {
            write_media(sink, media, "input_file", "file_url", "file_id")
        }
        ContentBlock::Audio(media) => write_audio(sink, media),
        ContentBlock::Refusal(text) => {
            sink.write_all(b"{\"refusal\":")?;
            write_json(sink, text.expose_for_wire())?;
            sink.write_all(b",\"type\":\"refusal\"}")
        }
        ContentBlock::ToolCall(_)
        | ContentBlock::ToolResult(_)
        | ContentBlock::Reasoning(_)
        | ContentBlock::ProviderExtension(_) => Err(invalid_io("non-message content was grouped")),
    }
}

fn write_media<S: WireSink>(
    sink: &mut S,
    media: &MediaInput,
    kind: &'static str,
    url_key: &'static str,
    file_key: &'static str,
) -> io::Result<()> {
    sink.write_all(b"{")?;
    if let Some(bytes) = media.inline_bytes_for_wire() {
        write_json(sink, if kind == "input_file" { "file_data" } else { url_key })?;
        sink.write_all(b":\"data:")?;
        sink.write_all(media.media_type().as_str().as_bytes())?;
        sink.write_all(b";base64,")?;
        sink.write_base64(bytes)?;
        sink.write_all(b"\"")?;
    } else if let Some((reference_kind, reference)) = media.reference_for_wire() {
        let key = match reference_kind {
            MediaReferenceKind::HttpsUrl => url_key,
            MediaReferenceKind::ProviderFile => file_key,
        };
        write_json(sink, key)?;
        sink.write_all(b":")?;
        write_json(sink, reference)?;
    } else {
        return Err(invalid_io("unresolved media reached OpenAI encoding"));
    }
    sink.write_all(b",\"type\":")?;
    write_json(sink, kind)?;
    sink.write_all(b"}")
}

fn write_audio<S: WireSink>(sink: &mut S, media: &MediaInput) -> io::Result<()> {
    let bytes = media
        .inline_bytes_for_wire()
        .ok_or_else(|| invalid_io("unresolved audio reached OpenAI encoding"))?;
    let format = match media.media_type().as_str() {
        "audio/wav" => "wav",
        "audio/mpeg" => "mp3",
        _ => return Err(invalid_io("unsupported audio reached OpenAI encoding")),
    };
    sink.write_all(b"{\"input_audio\":{\"data\":\"")?;
    sink.write_base64(bytes)?;
    sink.write_all(b"\",\"format\":")?;
    write_json(sink, format)?;
    sink.write_all(b"},\"type\":\"input_audio\"}")
}

fn write_standalone_item<S: WireSink>(sink: &mut S, block: &ContentBlock) -> io::Result<()> {
    match block {
        ContentBlock::ToolCall(call) => write_tool_call(sink, call),
        ContentBlock::ToolResult(result) => write_tool_result(sink, result),
        ContentBlock::Reasoning(replay) => write_reasoning_replay(sink, replay),
        ContentBlock::ProviderExtension(_) => {
            Err(invalid_io("provider extension reached OpenAI encoding"))
        }
        ContentBlock::Text(_)
        | ContentBlock::Image(_)
        | ContentBlock::Audio(_)
        | ContentBlock::Document(_)
        | ContentBlock::Refusal(_) => Err(invalid_io("message content was not grouped")),
    }
}

fn write_tool_call<S: WireSink>(sink: &mut S, call: &CompletedToolCall) -> io::Result<()> {
    let arguments = core::str::from_utf8(call.arguments().canonical_bytes())
        .map_err(|_| invalid_io("tool arguments were not UTF-8"))?;
    sink.write_all(b"{\"arguments\":")?;
    write_json(sink, arguments)?;
    sink.write_all(b",\"call_id\":")?;
    write_json(sink, call.id().expose_for_wire())?;
    sink.write_all(b",\"name\":")?;
    write_json(sink, call.name().as_str())?;
    sink.write_all(b",\"type\":\"function_call\"}")
}

fn write_tool_result<S: WireSink>(sink: &mut S, result: &ToolResult) -> io::Result<()> {
    sink.write_all(b"{\"call_id\":")?;
    write_json(sink, result.call_id().expose_for_wire())?;
    sink.write_all(b",\"output\":\"{\\\"is_error\\\":")?;
    write_bool(sink, result.is_error())?;
    sink.write_all(b",\\\"output\\\":")?;
    write_escaped_string_content(sink, result.output().canonical_bytes())?;
    sink.write_all(b"}\",\"type\":\"function_call_output\"}")
}

fn write_escaped_string_content<S: WireSink>(sink: &mut S, bytes: &[u8]) -> io::Result<()> {
    let mut start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        let replacement: &[u8] = match byte {
            b'"' => b"\\\"",
            b'\\' => b"\\\\",
            _ => continue,
        };
        sink.write_all(&bytes[start..index])?;
        sink.write_all(replacement)?;
        start = index + 1;
    }
    sink.write_all(&bytes[start..])
}

fn write_reasoning_replay<S: WireSink>(
    sink: &mut S,
    replay: &ReasoningReplay,
) -> io::Result<()> {
    let encrypted = core::str::from_utf8(replay.opaque_for_wire())
        .map_err(|_| invalid_io("reasoning replay was not UTF-8"))?;
    sink.write_all(b"{\"encrypted_content\":")?;
    write_json(sink, encrypted)?;
    sink.write_all(b",\"summary\":[")?;
    if let Some(summary) = replay.summary() {
        sink.write_all(b"{\"text\":")?;
        write_json(sink, summary.expose_for_wire())?;
        sink.write_all(b",\"type\":\"summary_text\"}")?;
    }
    sink.write_all(b"],\"type\":\"reasoning\"}")
}

fn write_tools<S: WireSink>(sink: &mut S, tools: &[ToolDefinition]) -> io::Result<()> {
    sink.write_all(b"[")?;
    for (index, tool) in tools.iter().enumerate() {
        if index != 0 {
            sink.write_all(b",")?;
        }
        sink.write_all(b"{\"description\":")?;
        write_optional_string(
            sink,
            tool.description().map(peritus_model_protocol::BoundedText::expose_for_wire),
        )?;
        sink.write_all(b",\"name\":")?;
        write_json(sink, tool.name().as_str())?;
        sink.write_all(b",\"parameters\":")?;
        sink.write_all(tool.parameters().canonical_bytes())?;
        sink.write_all(b",\"strict\":")?;
        write_bool(sink, tool.strict())?;
        sink.write_all(b",\"type\":\"function\"}")?;
    }
    sink.write_all(b"]")
}

fn write_tool_choice<S: WireSink>(sink: &mut S, choice: &ToolChoice) -> io::Result<()> {
    match choice {
        ToolChoice::Auto => write_json(sink, "auto"),
        ToolChoice::None => write_json(sink, "none"),
        ToolChoice::Required => write_json(sink, "required"),
        ToolChoice::Specific(name) => {
            sink.write_all(b"{\"name\":")?;
            write_json(sink, name.as_str())?;
            sink.write_all(b",\"type\":\"function\"}")
        }
    }
}

fn write_text_options<S: WireSink>(sink: &mut S, output: &StructuredOutput) -> io::Result<()> {
    sink.write_all(b"{\"format\":")?;
    match output {
        StructuredOutput::Text => sink.write_all(b"{\"type\":\"text\"}")?,
        StructuredOutput::JsonObject => sink.write_all(b"{\"type\":\"json_object\"}")?,
        StructuredOutput::JsonSchema { name, schema, strict } => {
            sink.write_all(b"{\"name\":")?;
            write_json(sink, name.as_str())?;
            sink.write_all(b",\"schema\":")?;
            sink.write_all(schema.canonical_bytes())?;
            sink.write_all(b",\"strict\":")?;
            write_bool(sink, *strict)?;
            sink.write_all(b",\"type\":\"json_schema\"}")?;
        }
    }
    sink.write_all(b"}")
}

fn write_reasoning_options<S: WireSink>(
    sink: &mut S,
    policy: ReasoningPolicy,
) -> io::Result<()> {
    match policy {
        ReasoningPolicy::Disabled => Err(invalid_io("disabled reasoning reached encoding")),
        ReasoningPolicy::Adaptive { summary } => {
            sink.write_all(b"{\"summary\":")?;
            write_optional_string(sink, summary_name(summary))?;
            sink.write_all(b"}")
        }
        ReasoningPolicy::Effort { effort, summary } => {
            sink.write_all(b"{\"effort\":")?;
            write_json(sink, effort_name(effort))?;
            sink.write_all(b",\"summary\":")?;
            write_optional_string(sink, summary_name(summary))?;
            sink.write_all(b"}")
        }
    }
}

fn includes_reasoning(request: &ModelRequest) -> bool {
    request.negotiated().includes(Capability::ReasoningReplay)
        || request.negotiated().includes(Capability::ReasoningControls)
        || request
            .messages()
            .iter()
            .flat_map(Message::content)
            .any(|block| matches!(block, ContentBlock::Reasoning(_)))
}

const fn role_name(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::Developer => "developer",
        Role::User | Role::Tool => "user",
        Role::Assistant => "assistant",
    }
}

const fn effort_name(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::Minimal => "minimal",
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::XHigh => "xhigh",
        ReasoningEffort::Max => "max",
        ReasoningEffort::Ultra => "ultra",
    }
}

const fn summary_name(summary: SummaryPolicy) -> Option<&'static str> {
    match summary {
        SummaryPolicy::None => None,
        SummaryPolicy::Auto => Some("auto"),
        SummaryPolicy::Concise => Some("concise"),
        SummaryPolicy::Detailed => Some("detailed"),
    }
}

fn write_optional_string<S: WireSink>(sink: &mut S, value: Option<&str>) -> io::Result<()> {
    match value {
        Some(value) => write_json(sink, value),
        None => sink.write_all(b"null"),
    }
}

fn write_bool<S: WireSink>(sink: &mut S, value: bool) -> io::Result<()> {
    sink.write_all(if value { b"true" } else { b"false" })
}

fn write_json<S: WireSink, T: ?Sized + Serialize>(sink: &mut S, value: &T) -> io::Result<()> {
    serde_json::to_writer(sink, value).map_err(json_io)
}

fn json_io(_error: serde_json::Error) -> io::Error {
    invalid_io("OpenAI JSON serialization failed")
}

fn invalid_io(detail: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, detail)
}

fn wire_error(exceeded: bool) -> ProviderCoreError {
    if exceeded {
        ProviderCoreError::limit_exceeded(
            "openai_request",
            "OpenAI wire request exceeds the selected HTTP body bound",
        )
    } else {
        error::invalid("OpenAI request serialization failed")
    }
}

fn base64_length(bytes: usize) -> Option<usize> {
    bytes
        .checked_div(3)
        .and_then(|groups| groups.checked_mul(4))
        .and_then(|complete| complete.checked_add(if bytes % 3 == 0 { 0 } else { 4 }))
}

trait WireSink: io::Write {
    fn write_base64(&mut self, bytes: &[u8]) -> io::Result<()>;
}

struct CountingSink {
    length: usize,
    maximum: usize,
    exceeded: bool,
}

impl CountingSink {
    const fn new(maximum: usize) -> Self {
        Self { length: 0, maximum, exceeded: false }
    }

    fn admit(&mut self, bytes: usize) -> io::Result<()> {
        let Some(next) = self.length.checked_add(bytes) else {
            self.exceeded = true;
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "wire size overflowed"));
        };
        if next > self.maximum {
            self.exceeded = true;
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "wire size exceeded"));
        }
        self.length = next;
        Ok(())
    }
}

impl io::Write for CountingSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.admit(bytes.len())?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl WireSink for CountingSink {
    fn write_base64(&mut self, bytes: &[u8]) -> io::Result<()> {
        let Some(length) = base64_length(bytes.len()) else {
            self.exceeded = true;
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "base64 size overflowed"));
        };
        self.admit(length)
    }
}

struct BodySink {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded: bool,
}

impl BodySink {
    fn new(maximum: usize) -> Self {
        Self { bytes: Vec::with_capacity(maximum), maximum, exceeded: false }
    }

    fn admit(&mut self, bytes: usize) -> io::Result<usize> {
        let Some(next) = self.bytes.len().checked_add(bytes) else {
            self.exceeded = true;
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "wire size overflowed"));
        };
        if next > self.maximum {
            self.exceeded = true;
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "wire size exceeded"));
        }
        Ok(next)
    }
}

impl io::Write for BodySink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.admit(bytes.len())?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl WireSink for BodySink {
    fn write_base64(&mut self, bytes: &[u8]) -> io::Result<()> {
        let Some(length) = base64_length(bytes.len()) else {
            self.exceeded = true;
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "base64 size overflowed"));
        };
        let next = self.admit(length)?;
        let start = self.bytes.len();
        self.bytes.resize(next, 0);
        let written = base64::engine::general_purpose::STANDARD
            .encode_slice(bytes, &mut self.bytes[start..])
            .map_err(|_| invalid_io("base64 encoding failed"))?;
        if written != length {
            return Err(invalid_io("base64 length changed"));
        }
        Ok(())
    }
}
