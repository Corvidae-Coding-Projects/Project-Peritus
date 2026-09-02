//! Bounded external projections of values retained exactly in the native trace.

use core::fmt::Write as _;

use sha2::{Digest as _, Sha256};

const ASSISTANT_PREVIEW_BYTES: usize = 384;
const TOOL_ARGUMENT_PREVIEW_BYTES: usize = 256;
const TOOL_OUTPUT_PREVIEW_BYTES: usize = 256;

pub(super) fn assistant(value: &str) -> String {
    project("assistant text", value, ASSISTANT_PREVIEW_BYTES)
}

pub(super) fn tool_arguments(value: &str) -> String {
    project("tool arguments", value, TOOL_ARGUMENT_PREVIEW_BYTES)
}

pub(super) fn tool_output(value: &str) -> String {
    project("tool output", value, TOOL_OUTPUT_PREVIEW_BYTES)
}

fn project(kind: &str, value: &str, preview_bytes: usize) -> String {
    if value.len() <= preview_bytes {
        return value.to_owned();
    }

    let head_budget = preview_bytes.saturating_mul(2) / 3;
    let tail_budget = preview_bytes.saturating_sub(head_budget);
    let head = prefix(value, head_budget);
    let tail = suffix(value, tail_budget);
    let digest = Sha256::digest(value.as_bytes());
    let mut projected = String::new();
    let _ = write!(projected, "[Peritus bounded {kind}: original_bytes={} sha256=", value.len());
    for byte in digest {
        let _ = write!(projected, "{byte:02x}");
    }
    projected.push_str("]\n");
    projected.push_str(head);
    projected.push_str("\n...[middle omitted; exact value is in the native trace]...\n");
    projected.push_str(tail);
    projected
}

fn prefix(value: &str, budget: usize) -> &str {
    let mut end = budget.min(value.len());
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    &value[..end]
}

fn suffix(value: &str, budget: usize) -> &str {
    let mut start = value.len().saturating_sub(budget);
    while !value.is_char_boundary(start) {
        start = start.saturating_add(1);
    }
    &value[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_values_remain_exact() {
        assert_eq!(tool_output("complete"), "complete");
    }

    #[test]
    fn long_utf8_values_are_digest_labeled_and_bounded() {
        let value = "évidence-".repeat(200);
        let projected = tool_output(&value);

        assert!(projected.contains("Peritus bounded tool output"));
        assert!(projected.contains(&format!("original_bytes={}", value.len())));
        assert!(projected.contains("exact value is in the native trace"));
        assert!(projected.len() < 512);
    }
}
