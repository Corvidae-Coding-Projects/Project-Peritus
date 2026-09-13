//! Exact byte model of the deterministic decision fingerprint.

use super::{EvidenceAssessment, FindingAssessment, QualificationAssessment, ReleaseVerdict, ReviewAssessment};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

pub open spec fn low_u16(value: u16) -> u8 { (value % 256) as u8 }

pub open spec fn bool_byte(value: bool) -> u8 { if value { 1 } else { 0 } }

pub open spec fn evidence_step(
    bytes: Seq<u8>,
    assessment: EvidenceAssessment,
    index: nat,
) -> Seq<u8> {
    let slot = index % 32;
    let digest = assessment.spec_contributing_digest().spec_bytes()@;
    let with_digest = bytes.update(slot as int, bytes[slot as int] ^ digest[slot as int]);
    let count_slot = (slot + 7) % 32;
    let with_count = with_digest.update(
        count_slot as int,
        with_digest[count_slot as int] ^ low_u16(assessment.spec_contributing_count()),
    );
    let ready_slot = (slot + 13) % 32;
    with_count.update(
        ready_slot as int,
        with_count[ready_slot as int] ^ bool_byte(assessment.spec_is_satisfied()),
    )
}

pub open spec fn evidence_through(
    initial: Seq<u8>,
    evidence: Seq<EvidenceAssessment>,
    end: nat,
) -> Seq<u8>
    decreases end,
{
    if end == 0 {
        initial
    } else {
        evidence_step(
            evidence_through(initial, evidence, (end - 1) as nat),
            evidence[(end - 1) as int],
            (end - 1) as nat,
        )
    }
}

pub open spec fn qualification_step(
    bytes: Seq<u8>,
    assessment: QualificationAssessment,
    index: nat,
) -> Seq<u8> {
    let slot = 24 + index;
    let digest = assessment.spec_report_digest().spec_bytes()@;
    let with_digest = bytes.update(slot as int, bytes[slot as int] ^ digest[slot as int]);
    with_digest.update(
        (slot + 4) as int,
        with_digest[(slot + 4) as int] ^ bool_byte(assessment.spec_is_satisfied()),
    )
}

pub open spec fn qualifications_through(
    initial: Seq<u8>,
    qualifications: Seq<QualificationAssessment>,
    end: nat,
) -> Seq<u8>
    decreases end,
{
    if end == 0 {
        initial
    } else {
        qualification_step(
            qualifications_through(initial, qualifications, (end - 1) as nat),
            qualifications[(end - 1) as int],
            (end - 1) as nat,
        )
    }
}

pub open spec fn verdict_byte(verdict: ReleaseVerdict) -> u8 {
    if verdict == ReleaseVerdict::Ready { 0xA5 } else { 0x5A }
}

pub open spec fn finish_digest(
    bytes: Seq<u8>,
    verdict: ReleaseVerdict,
    reviews: ReviewAssessment,
    findings: FindingAssessment,
) -> Seq<u8> {
    let verdict_bytes = bytes.update(0, bytes[0] ^ verdict_byte(verdict));
    let review_count = verdict_bytes.update(
        1, verdict_bytes[1] ^ low_u16(reviews.spec_approved_count()));
    let review_ready = review_count.update(
        2, review_count[2] ^ bool_byte(reviews.spec_is_satisfied()));
    let finding_open = review_ready.update(
        3, review_ready[3] ^ low_u16(findings.spec_open_count()));
    let finding_blocking = finding_open.update(
        4, finding_open[4] ^ low_u16(findings.spec_release_blocking_count()));
    finding_blocking.update(
        5, finding_blocking[5] ^ bool_byte(findings.spec_is_satisfied()))
}

/// Exact output bytes for the production decision-fingerprint reducer.
pub open spec fn expected_decision_digest(
    manifest: Sha256Digest,
    verdict: ReleaseVerdict,
    evidence: Seq<EvidenceAssessment>,
    qualifications: Seq<QualificationAssessment>,
    reviews: ReviewAssessment,
    findings: FindingAssessment,
) -> Seq<u8> {
    finish_digest(
        qualifications_through(
            evidence_through(manifest.spec_bytes()@, evidence, evidence.len() as nat),
            qualifications,
            qualifications.len() as nat,
        ),
        verdict,
        reviews,
        findings,
    )
}

} // verus!
