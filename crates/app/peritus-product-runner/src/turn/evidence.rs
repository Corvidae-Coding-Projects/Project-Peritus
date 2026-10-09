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
    let allocations = allocations(&values, evidence_budget(max_input_tokens, framing_tokens));
    let [transcript, diff, gates, developer, prior, correction] = std::array::from_fn(|index| {
        bounded(SOURCE_NAMES[index], values[index], allocations[index])
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

fn digest_hex(value: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_bounds_first_turn_and_preserves_each_section() {
        let transcript = "literal request";
        let diff = format!("diff-head{}diff-tail", "d".repeat(700_000));
        let gates = "gate evidence";
        let developer = format!("command-head{}command-tail", "e".repeat(500_000));
        let prior = "conserved finding";
        let correction = "retry correction";

        let projected =
            project(200_000, 0, [transcript, &diff, gates, &developer, prior, correction]);
        let total = projected.transcript.len()
            + projected.diff.len()
            + projected.gates.len()
            + projected.developer.len()
            + projected.prior.len()
            + projected.correction.len();

        assert!(total <= evidence_budget(200_000, 0));
        assert_eq!(projected.transcript, transcript);
        assert_eq!(projected.gates, gates);
        assert_eq!(projected.prior, prior);
        assert_eq!(projected.correction, correction);
        assert!(projected.diff.starts_with("diff-head"));
        assert!(projected.diff.ends_with("diff-tail"));
        assert!(projected.developer.starts_with("command-head"));
        assert!(projected.developer.ends_with("command-tail"));
        assert!(projected.diff.contains("sha256="));
        assert!(projected.diff.contains("source=diff"));
        assert!(projected.diff.contains("exact omitted UTF-8 byte range=["));
        assert!(projected.developer.contains("section=developer_commands"));
    }

    #[test]
    fn smaller_provider_profiles_receive_smaller_evidence_packets() {
        let content = "x".repeat(900_000);
        let smaller = project(64_000, 0, ["task", &content, &content, &content, "", ""]);
        let larger = project(200_000, 0, ["task", &content, &content, &content, "", ""]);
        let smaller_total = smaller.diff.len() + smaller.gates.len() + smaller.developer.len();
        let larger_total = larger.diff.len() + larger.gates.len() + larger.developer.len();

        assert!(smaller_total < larger_total);
        assert!(smaller_total <= evidence_budget(64_000, 0));
    }

    #[test]
    fn omission_marker_identifies_the_exact_utf8_source_interval() {
        let source = format!("leading🌱{}trailing", "record🌱".repeat(10_000));
        let excerpt = bounded("transcript", &source, 4_096);
        let marker_start = excerpt.find("[Peritus reviewer source=").expect("marker start");
        let marker_end =
            excerpt[marker_start..].find("]\n").expect("marker end") + marker_start + 2;
        let marker = &excerpt[marker_start..marker_end];
        let range = marker
            .split("exact omitted UTF-8 byte range=[")
            .nth(1)
            .expect("range")
            .split(").")
            .next()
            .expect("range end");
        let (start, end) = range.split_once(',').expect("range bounds");
        let start = start.parse::<usize>().expect("range start");
        let end = end.parse::<usize>().expect("range end");

        assert!(source.is_char_boundary(start));
        assert!(source.is_char_boundary(end));
        assert_eq!(&source[..start], &excerpt[..marker_start - 1]);
        assert_eq!(&source[end..], &excerpt[marker_end..]);
        assert!(marker.contains("section=transcript"));
        assert!(marker.contains(&format!("original_bytes={}", source.len())));
    }

    #[test]
    fn evidence_uses_actual_input_headroom_after_measured_framing() {
        let content = "x".repeat(100_000);
        let available = project(20_000, 3_000, ["", &content, "", "", "", ""]);
        let unavailable = project(3_100, 3_000, ["", &content, "", "", "", ""]);
        assert_eq!(available.diff.len(), 51_000);
        assert!(unavailable.diff.len() < available.diff.len());
        assert!(unavailable.diff.contains("sha256="));
    }
}
