//! Canonical schema-v1 report encoding.

use crate::ReportId;
use peritus_types::Sha256Digest;

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
