//! Canonical schema-v1 report encoding.

use crate::{AlternativeCauses, EvidenceCitation, ReportId};
use peritus_types::Sha256Digest;

use super::claim::ClaimContent;

pub(super) fn encode_report_page(
    report_id: ReportId,
    payload_digest: Sha256Digest,
    ordinal: u64,
    start: u64,
    end: u64,
    payload: &[u8],
) -> (Vec<u8>, core::ops::Range<usize>) {
    let mut bytes = b"peritus-e2-debugger-report-page-v2\0".to_vec();
    bytes.extend_from_slice(&2_u16.to_be_bytes());
    bytes.extend_from_slice(report_id.as_bytes());
    bytes.extend_from_slice(payload_digest.as_bytes());
    bytes.extend_from_slice(&ordinal.to_be_bytes());
    bytes.extend_from_slice(&start.to_be_bytes());
    bytes.extend_from_slice(&end.to_be_bytes());
    let payload_start = bytes.len().saturating_add(core::mem::size_of::<u64>());
    crate::query::encode_blob(&mut bytes, payload);
    let payload_end = bytes.len();
    (bytes, payload_start..payload_end)
}

pub(super) fn encode_report_index(
    report_id: ReportId,
    payload_digest: Sha256Digest,
    payload_size: u64,
    level: u16,
    ordinal: u64,
    range: (u64, u64),
    children: &[(Sha256Digest, u64, u64, u64)],
) -> Vec<u8> {
    let mut bytes = b"peritus-e2-debugger-report-index-v2\0".to_vec();
    bytes.extend_from_slice(&2_u16.to_be_bytes());
    bytes.extend_from_slice(report_id.as_bytes());
    bytes.extend_from_slice(payload_digest.as_bytes());
    bytes.extend_from_slice(&payload_size.to_be_bytes());
    bytes.extend_from_slice(&level.to_be_bytes());
    bytes.extend_from_slice(&ordinal.to_be_bytes());
    bytes.extend_from_slice(&range.0.to_be_bytes());
    bytes.extend_from_slice(&range.1.to_be_bytes());
    crate::query::encode_len(&mut bytes, children.len());
    for (digest, size, start, end) in children {
        bytes.extend_from_slice(digest.as_bytes());
        bytes.extend_from_slice(&size.to_be_bytes());
        bytes.extend_from_slice(&start.to_be_bytes());
        bytes.extend_from_slice(&end.to_be_bytes());
    }
    bytes
}

pub(super) fn encode_claim_content(bytes: &mut Vec<u8>, content: &ClaimContent) {
    match content {
        ClaimContent::Observation { statement, support } => {
            bytes.push(1);
            encode_text(bytes, statement);
            encode_citations(bytes, support);
        }
        ClaimContent::Inference {
            statement,
            support,
            contrary,
            alternatives,
            confidence,
            category,
        } => {
            bytes.push(2);
            encode_text(bytes, statement);
            encode_citations(bytes, support);
            encode_citations(bytes, contrary);
            encode_alternatives(bytes, alternatives);
            bytes.extend_from_slice(&confidence.value().to_be_bytes());
            encode_confidence_basis(bytes, confidence.basis());
            bytes.extend_from_slice(&category.tag().to_be_bytes());
        }
        ClaimContent::Recommendation { statement, support, parent, affected_components } => {
            bytes.push(3);
            encode_text(bytes, statement);
            encode_citations(bytes, support);
            bytes.extend_from_slice(parent.as_bytes());
            crate::query::encode_len(bytes, affected_components.len());
            for component in affected_components {
                bytes.push(component.tag());
            }
        }
        ClaimContent::Unsupported(value) => {
            bytes.push(4);
            bytes.extend_from_slice(value.proposal_digest().as_bytes());
            bytes.push(value.reason() as u8);
        }
    }
}

fn encode_text(bytes: &mut Vec<u8>, text: &crate::DiagnosticText) {
    crate::query::encode_blob(bytes, text.as_str().as_bytes());
}
fn encode_citations(bytes: &mut Vec<u8>, values: &[EvidenceCitation]) {
    crate::query::encode_len(bytes, values.len());
    for value in values {
        value.encode(bytes);
    }
}
fn encode_alternatives(bytes: &mut Vec<u8>, value: &AlternativeCauses) {
    match value {
        AlternativeCauses::NoneKnown => bytes.push(0),
        AlternativeCauses::Categories(values) => {
            bytes.push(1);
            crate::query::encode_len(bytes, values.len());
            for value in values {
                bytes.extend_from_slice(&value.tag().to_be_bytes());
            }
        }
    }
}
fn encode_confidence_basis(bytes: &mut Vec<u8>, value: crate::ConfidenceBasis) {
    for count in [
        value.support_count(),
        value.contrary_count(),
        value.ambiguity_count(),
        value.recurrence_count(),
        value.maximum_causal_distance(),
    ] {
        bytes.extend_from_slice(&count.to_be_bytes());
    }
}
