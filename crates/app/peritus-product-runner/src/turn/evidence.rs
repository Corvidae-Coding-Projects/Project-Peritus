//! Provider-aware projection of the initial independent-review evidence packet.

use std::fmt::Write as _;

use sha2::{Digest as _, Sha256};

const TOKEN_ESTIMATE_BYTES: u64 = 3;
const REVIEW_INPUT_SHARE_PERCENT: u64 = 50;
const MAX_REVIEW_EVIDENCE_BYTES: usize = 384 * 1024;
const BASE_SECTION_CAPS: [usize; 6] =
    [64 * 1024, 144 * 1024, 48 * 1024, 80 * 1024, 32 * 1024, 16 * 1024];
const BASE_TOTAL_BYTES: usize = 384 * 1024;
const EXTRA_PRIORITY: [usize; 6] = [0, 1, 3, 2, 4, 5];

pub struct ReviewerPrompt<'a> {
    pub transcript: &'a str,
    pub diff: &'a str,
    pub gates: &'a str,
    pub developer_evidence: &'a str,
    pub prior: &'a str,
    pub max_input_tokens: u64,
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
    transcript: &str,
    diff: &str,
    gates: &str,
    developer: &str,
    prior: &str,
    correction: &str,
) -> ReviewerEvidence {
    let values = [transcript, diff, gates, developer, prior, correction];
    let allocations = allocations(&values, evidence_budget(max_input_tokens));
    let [transcript, diff, gates, developer, prior, correction] =
        std::array::from_fn(|index| bounded(values[index], allocations[index]));
    ReviewerEvidence { transcript, diff, gates, developer, prior, correction }
}

fn evidence_budget(max_input_tokens: u64) -> usize {
    let bytes = max_input_tokens
        .saturating_mul(TOKEN_ESTIMATE_BYTES)
        .saturating_mul(REVIEW_INPUT_SHARE_PERCENT)
        / 100;
    usize::try_from(bytes).unwrap_or(usize::MAX).min(MAX_REVIEW_EVIDENCE_BYTES)
}

fn allocations(values: &[&str; 6], budget: usize) -> [usize; 6] {
    let mut allocated = std::array::from_fn(|index| {
        let weighted = budget.saturating_mul(BASE_SECTION_CAPS[index]) / BASE_TOTAL_BYTES;
        values[index].len().min(weighted)
    });
    let mut remaining = budget.saturating_sub(allocated.iter().sum());
    for index in EXTRA_PRIORITY {
        let available = values[index].len().saturating_sub(allocated[index]);
        let additional = available.min(remaining);
        allocated[index] = allocated[index].saturating_add(additional);
        remaining -= additional;
    }
    allocated
}

fn bounded(value: &str, maximum: usize) -> String {
    if value.len() <= maximum {
        return value.to_owned();
    }
    let digest = digest_hex(value);
    let marker = format!(
        "\n[Peritus bounded reviewer evidence: original_bytes={} sha256={digest}; middle omitted. Use fresh read-only workspace tools for authoritative current detail.]\n",
        value.len(),
    );
    if maximum <= marker.len() {
        return marker[..maximum].to_owned();
    }
    let retained = maximum - marker.len();
    let head_end = value.floor_char_boundary(retained.saturating_mul(2) / 3);
    let tail_bytes = retained - head_end;
    let tail_start = suffix_boundary(value, tail_bytes);
    format!("{}{marker}{}", &value[..head_end], &value[tail_start..])
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

        let projected = project(200_000, transcript, &diff, gates, &developer, prior, correction);
        let total = projected.transcript.len()
            + projected.diff.len()
            + projected.gates.len()
            + projected.developer.len()
            + projected.prior.len()
            + projected.correction.len();

        assert!(total <= evidence_budget(200_000));
        assert_eq!(projected.transcript, transcript);
        assert_eq!(projected.gates, gates);
        assert_eq!(projected.prior, prior);
        assert_eq!(projected.correction, correction);
        assert!(projected.diff.starts_with("diff-head"));
        assert!(projected.diff.ends_with("diff-tail"));
        assert!(projected.developer.starts_with("command-head"));
        assert!(projected.developer.ends_with("command-tail"));
        assert!(projected.diff.contains("sha256="));
        assert!(projected.developer.contains("middle omitted"));
    }

    #[test]
    fn smaller_provider_profiles_receive_smaller_evidence_packets() {
        let content = "x".repeat(900_000);
        let smaller = project(64_000, "task", &content, &content, &content, "", "");
        let larger = project(200_000, "task", &content, &content, &content, "", "");
        let smaller_total = smaller.diff.len() + smaller.gates.len() + smaller.developer.len();
        let larger_total = larger.diff.len() + larger.gates.len() + larger.developer.len();

        assert!(smaller_total < larger_total);
        assert!(smaller_total <= evidence_budget(64_000));
    }
}
