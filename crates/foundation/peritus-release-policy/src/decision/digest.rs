//! Deterministic fingerprint reduction over canonical assessments.

use super::{
    DecisionDigest, EvidenceAssessment, FindingAssessment, QualificationAssessment, ReleaseVerdict,
    ReviewAssessment,
};
use peritus_types::Sha256Digest;

#[allow(
    clippy::cast_possible_truncation,
    reason = "modulo 256 establishes the exact low-byte range before conversion"
)]
const fn low_u16_byte(value: u16) -> u8 {
    (value % 256) as u8
}
const fn bool_byte(value: bool) -> u8 {
    if value { 1 } else { 0 }
}
pub(super) const fn decision_digest(
    manifest: Sha256Digest,
    verdict: ReleaseVerdict,
    evidence: &[EvidenceAssessment; 44],
    qualifications: &[QualificationAssessment; 4],
    reviews: ReviewAssessment,
    findings: FindingAssessment,
) -> DecisionDigest {
    let mut bytes = *manifest.as_bytes();
    let mut index = 0;
    while index < evidence.len() {
        let assessment = evidence[index];
        let source = assessment.contributing_digest();
        let slot = index % bytes.len();
        bytes[slot] ^= source.as_bytes()[slot];
        bytes[(slot + 7) % 32] ^= low_u16_byte(assessment.contributing_count());
        bytes[(slot + 13) % 32] ^= bool_byte(assessment.is_satisfied());
        index += 1;
    }
    let mut qualification_index = 0;
    while qualification_index < qualifications.len() {
        let assessment = qualifications[qualification_index];
        let slot = 24 + qualification_index;
        bytes[slot] ^= assessment.report_digest().as_bytes()[slot];
        bytes[slot + 4] ^= bool_byte(assessment.is_satisfied());
        qualification_index += 1;
    }
    bytes[0] ^= match verdict {
        ReleaseVerdict::Ready => 0xA5,
        ReleaseVerdict::NotReadyForProduction => 0x5A,
    };
    bytes[1] ^= low_u16_byte(reviews.approved_count());
    bytes[2] ^= bool_byte(reviews.is_satisfied());
    bytes[3] ^= low_u16_byte(findings.open_count());
    bytes[4] ^= low_u16_byte(findings.release_blocking_count());
    bytes[5] ^= bool_byte(findings.is_satisfied());

    DecisionDigest::new(bytes)
}
