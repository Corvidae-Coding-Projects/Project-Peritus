//! Provider-aware projection of the initial independent-review evidence packet.

use std::fmt::Write as _;

use sha2::{Digest as _, Sha256};

const TOKEN_ESTIMATE_BYTES: u64 = 3;
const EXTRA_PRIORITY: [usize; 6] = [0, 1, 3, 2, 4, 5];
const SOURCE_NAMES: [&str; 6] =
    ["transcript", "diff", "gates", "developer_commands", "prior", "correction"];

pub struct ReviewerPrompt<'a> {
    pub system: &'a str,
    pub tools: &'a [peritus_model_protocol::ToolDefinition],
    pub transcript: &'a str,
    pub diff: &'a str,
    pub gates: &'a str,
    pub developer_evidence: &'a str,
    pub prior: &'a str,
    pub max_input_tokens: u64,
    pub additional_framing_tokens: u64,
    pub delivery: super::ReviewDelivery,
    pub correction: Option<&'a str>,
}

pub(super) struct ReviewerEvidence {
    pub(super) transcript: String,
    pub(super) diff: String,
    pub(super) gates: String,
    pub(super) developer: String,
    pub(super) prior: String,
    pub(super) correction: String,
}

pub(super) fn project(
    max_input_tokens: u64,
    framing_tokens: u64,
    values: [&str; 6],
) -> ReviewerEvidence {
    // Retained evidence from earlier versions or command output can contain NUL even when
    // it is valid UTF-8. Preserve its exact source identity and paged retrieval, but never
    // embed binary bytes in a model text block. Charge the marker's actual size to the view.
    let binary_markers: [Option<String>; 6] = std::array::from_fn(|index| {
        values[index].contains('\0').then(|| {
            format!(
                "[NUL-bearing evidence; inline preview omitted.]{}",
                omission_marker(
                    SOURCE_NAMES[index],
                    values[index].len(),
                    &digest_hex(values[index]),
                    0,
                    values[index].len(),
                )
            )
        })
    });
    let displayed: [&str; 6] =
        std::array::from_fn(|index| binary_markers[index].as_deref().unwrap_or(values[index]));
    let allocations = allocations(&displayed, evidence_budget(max_input_tokens, framing_tokens));
    let [transcript, diff, gates, developer, prior, correction] = std::array::from_fn(|index| {
        binary_markers[index].as_ref().map_or_else(
            || bounded(SOURCE_NAMES[index], values[index], allocations[index]),
            |marker| marker[..marker.len().min(allocations[index])].to_owned(),
        )
    });
    ReviewerEvidence { transcript, diff, gates, developer, prior, correction }
}

pub(super) const fn request_target(max_input_tokens: u64) -> u64 {
    max_input_tokens
}

pub(super) fn first_tool_result_reserve_tokens(
    tools: &[peritus_model_protocol::ToolDefinition],
) -> Result<u64, peritus_agent::DeveloperLoopError> {
    use peritus_model_protocol::{
        CanonicalJson, CompletedToolCall, ContentBlock, JsonBounds, Message, ProtocolLimits, Role,
        ToolCallId, ToolName, ToolResult,
    };

    let limits = ProtocolLimits::PRODUCTION;
    let maximum = crate::developer_tools::DEFAULT_INSPECTION_PAGE_BYTES;
    let make_output = |byte_count: usize| {
        serde_json::json!({
            "bytes": "x".repeat(byte_count),
            "next_offset": maximum,
            "offset": 0,
            "section": "transcript",
            "sha256": "0".repeat(64),
            "total_bytes": maximum.saturating_mul(2),
        })
    };
    let encoded_size = |value: &serde_json::Value| {
        serde_json::to_vec(value)
            .map(|bytes| bytes.len())
            .map_err(|error| peritus_agent::DeveloperLoopError::Context(error.to_string()))
    };
    let mut low = 0;
    let mut high = maximum;
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if encoded_size(&make_output(middle))? <= maximum {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let output_text = serde_json::to_string(&make_output(low))
        .map_err(|error| peritus_agent::DeveloperLoopError::Context(error.to_string()))?;
    let output = CanonicalJson::parse(&output_text, JsonBounds::value(limits))?;
    let call_id = ToolCallId::new("reviewer-first-page".to_owned())?;
    let arguments = CanonicalJson::parse(
        r#"{"depth":1,"max_bytes":16384,"path":"."}"#,
        JsonBounds::value(limits),
    )?;
    let call = CompletedToolCall::new(
        call_id.clone(),
        ToolName::new("workspace_list".to_owned())?,
        arguments,
    )?;
    let call_message = Message::new(Role::Assistant, vec![ContentBlock::ToolCall(call)], limits)?;
    let result = ToolResult::new(call_id, output, false);
    let result_message = Message::new(Role::Tool, vec![ContentBlock::ToolResult(result)], limits)?;
    let exchange = [call_message, result_message];
    let exchange_tokens = peritus_agent::estimate_developer_request_tokens(&exchange, tools);
    let tool_catalog_tokens = peritus_agent::estimate_developer_request_tokens(&[], tools);
    Ok(exchange_tokens.saturating_sub(tool_catalog_tokens))
}

fn evidence_budget(max_input_tokens: u64, framing_tokens: u64) -> usize {
    let tokens = request_target(max_input_tokens).saturating_sub(framing_tokens);
    usize::try_from(tokens.saturating_mul(TOKEN_ESTIMATE_BYTES)).unwrap_or(usize::MAX)
}

fn allocations(values: &[&str; 6], budget: usize) -> [usize; 6] {
    let mut allocated = [0; 6];
    let mut remaining = budget;
    let sections = EXTRA_PRIORITY.len();
    for index in EXTRA_PRIORITY {
        let available = values[index].len();
        let additional = available.min(remaining / sections);
        allocated[index] = additional;
        remaining = remaining.saturating_sub(additional);
    }
    for index in EXTRA_PRIORITY {
        let additional = values[index].len().saturating_sub(allocated[index]).min(remaining);
        allocated[index] = allocated[index].saturating_add(additional);
        remaining = remaining.saturating_sub(additional);
    }
    allocated
}

fn bounded(source: &str, value: &str, maximum: usize) -> String {
    if value.len() <= maximum {
        return value.to_owned();
    }
    let digest = digest_hex(value);
    let mut marker = omission_marker(source, value.len(), &digest, 0, value.len());
    if maximum <= marker.len() {
        return marker[..maximum].to_owned();
    }
    for _ in 0..4 {
        let retained = maximum.saturating_sub(marker.len());
        let head_end = value.floor_char_boundary(retained.saturating_mul(2) / 3);
        let tail_start = suffix_boundary(value, retained.saturating_sub(head_end));
        let next = omission_marker(source, value.len(), &digest, head_end, tail_start);
        let stable_length = next.len() == marker.len();
        marker = next;
        if stable_length {
            break;
        }
    }
    let retained = maximum.saturating_sub(marker.len());
    let head_end = value.floor_char_boundary(retained.saturating_mul(2) / 3);
    let tail_start = suffix_boundary(value, retained.saturating_sub(head_end));
    marker = omission_marker(source, value.len(), &digest, head_end, tail_start);
    format!("{}{marker}{}", &value[..head_end], &value[tail_start..])
}

fn omission_marker(source: &str, total: usize, digest: &str, start: usize, end: usize) -> String {
    format!(
        "\n[Peritus reviewer source={source}: original_bytes={total} sha256={digest}; exact omitted UTF-8 byte range=[{start},{end}). Retrieve this historical source with developer_evidence_read section={source}, offset pages; do not substitute current workspace files.]\n"
    )
}

pub(super) fn digest_hex(value: &str) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(value.as_bytes()) {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

const fn suffix_boundary(value: &str, bytes: usize) -> usize {
    let mut boundary = value.len().saturating_sub(bytes);
    while boundary < value.len() && !value.is_char_boundary(boundary) {
        boundary += 1;
    }
    boundary
}
