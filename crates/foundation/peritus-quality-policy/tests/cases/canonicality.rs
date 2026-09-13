use crate::support::{Fixture, bytes, digest};
use peritus_quality_policy::{
    AcceptanceEvidence, ApprovalObservation, ApprovalOutcome, ApprovalSubject,
    CanonicalEvidenceCollection, EvidenceError, EvidenceErrorKind, FindingDisposition,
    FindingObservation, GateOutcome, ReviewCycleOrdinal, ReviewObservation, ReviewerIdentity,
    WaiverObservation,
};
use peritus_spec::{FindingSeverity, ReviewCategory};
use peritus_types::{ActorId, ApprovalRequestId, FindingId, ReviewCycleId, Sha256Digest};

fn review(
    fixture: &Fixture,
    categories: Vec<ReviewCategory>,
    findings: Vec<FindingObservation>,
) -> Result<ReviewObservation, EvidenceError> {
    ReviewObservation::new(
        ReviewCycleId::new(bytes(70)).unwrap(),
        ReviewCycleOrdinal::new(1).unwrap(),
        fixture.revision(),
        ReviewerIdentity::new(
            ActorId::new(bytes(80)).unwrap(),
            digest(81),
            digest(82),
            digest(83),
            digest(84),
            digest(85),
            true,
        ),
        categories,
        findings,
        digest(86),
    )
}

fn finding(id: [u8; 16], disposition: FindingDisposition) -> FindingObservation {
    FindingObservation::new(
        FindingId::new(id).unwrap(),
        FindingSeverity::High,
        disposition,
        digest(50),
    )
}

fn assert_error(
    error: EvidenceError,
    kind: EvidenceErrorKind,
    collection: CanonicalEvidenceCollection,
    index: usize,
) {
    assert_eq!((error.kind(), error.collection(), error.index()), (kind, collection, index));
}

#[test]
fn category_order_uses_every_byte_and_keeps_duplicate_vs_descending_errors() {
    let fixture = Fixture::new();
    for position in 0..32 {
        let low = ReviewCategory::new(Sha256Digest::new([17; 32]));
        let mut high_bytes = [17; 32];
        high_bytes[position] = 18;
        let high = ReviewCategory::new(Sha256Digest::new(high_bytes));
        let ordered = review(&fixture, vec![low, high], Vec::new()).unwrap();
        assert_eq!(ordered.categories(), &[low, high]);
        assert_error(
            review(&fixture, vec![low, low], Vec::new()).unwrap_err(),
            EvidenceErrorKind::DuplicateObservation,
            CanonicalEvidenceCollection::ReviewCategories,
            1,
        );
        assert_error(
            review(&fixture, vec![high, low], Vec::new()).unwrap_err(),
            EvidenceErrorKind::NonCanonicalOrder,
            CanonicalEvidenceCollection::ReviewCategories,
            1,
        );
        assert_error(
            review(&fixture, vec![low, high, low], Vec::new()).unwrap_err(),
            EvidenceErrorKind::NonCanonicalOrder,
            CanonicalEvidenceCollection::ReviewCategories,
            2,
        );
    }
}

#[test]
fn finding_order_uses_every_byte_and_preserves_supplied_records() {
    let fixture = Fixture::new();
    for position in 0..16 {
        let low = finding([17; 16], FindingDisposition::Open);
        let mut high_bytes = [17; 16];
        high_bytes[position] = 18;
        let high = finding(
            high_bytes,
            FindingDisposition::Resolved {
                revision: fixture.revision(),
                evidence_digest: digest(91),
            },
        );
        let ordered = review(&fixture, vec![fixture.category_a], vec![low, high]).unwrap();
        assert_eq!(ordered.findings(), &[low, high]);
        assert_eq!(ordered.revision(), fixture.revision());
        assert_eq!(ordered.review_digest(), digest(86));
        assert_error(
            review(&fixture, vec![fixture.category_a], vec![high, low]).unwrap_err(),
            EvidenceErrorKind::NonCanonicalOrder,
            CanonicalEvidenceCollection::Findings,
            1,
        );
        assert_error(
            review(&fixture, vec![fixture.category_a], vec![low, low]).unwrap_err(),
            EvidenceErrorKind::DuplicateObservation,
            CanonicalEvidenceCollection::Findings,
            1,
        );
    }
}

#[test]
fn resolution_freshness_covers_every_component_and_preserves_validation_precedence() {
    let fixture = Fixture::new();
    for dimension in 0..7 {
        let mut parts = [1, 2, 3, 1, 1, 4, 5];
        parts[dimension] += 1;
        let stale = finding(
            [17; 16],
            FindingDisposition::Resolved {
                revision: fixture.revision_from(parts),
                evidence_digest: digest(91),
            },
        );
        assert_error(
            review(&fixture, vec![fixture.category_a], vec![stale]).unwrap_err(),
            EvidenceErrorKind::ResolutionRevisionMismatch,
            CanonicalEvidenceCollection::Findings,
            0,
        );
        assert_error(
            review(&fixture, vec![fixture.category_a, fixture.category_a], vec![stale])
                .unwrap_err(),
            EvidenceErrorKind::DuplicateObservation,
            CanonicalEvidenceCollection::ReviewCategories,
            1,
        );
        let same_id = finding([17; 16], FindingDisposition::Open);
        assert_error(
            review(&fixture, vec![fixture.category_a], vec![same_id, stale]).unwrap_err(),
            EvidenceErrorKind::ResolutionRevisionMismatch,
            CanonicalEvidenceCollection::Findings,
            1,
        );
    }
    assert!(review(&fixture, vec![fixture.category_a], Vec::new()).is_ok());
}

fn approval(fixture: &Fixture, request: u8, subject: ApprovalSubject) -> ApprovalObservation {
    ApprovalObservation::new(
        ApprovalRequestId::new(bytes(request)).unwrap(),
        fixture.revision(),
        subject,
        ActorId::new(bytes(80)).unwrap(),
        fixture.waiver_authority,
        ApprovalOutcome::Approved,
        digest(91),
    )
}

#[test]
fn nonadjacent_approval_subjects_compare_every_finding_byte() {
    let fixture = Fixture::new();
    for position in 0..16 {
        let low = FindingId::new([17; 16]).unwrap();
        let mut high_bytes = [17; 16];
        high_bytes[position] = 18;
        let high = FindingId::new(high_bytes).unwrap();
        let first = approval(&fixture, 90, ApprovalSubject::FindingWaiver(low));
        let middle = approval(&fixture, 91, ApprovalSubject::Acceptance);
        let last = approval(&fixture, 92, ApprovalSubject::FindingWaiver(high));
        let accepted = AcceptanceEvidence::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![first, middle, last],
            Vec::new(),
        )
        .unwrap();
        assert_eq!(accepted.approvals(), &[first, middle, last]);
        let duplicate = approval(&fixture, 92, ApprovalSubject::FindingWaiver(low));
        assert_error(
            AcceptanceEvidence::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                vec![first, middle, duplicate],
                Vec::new(),
            )
            .unwrap_err(),
            EvidenceErrorKind::DuplicateApprovalSubject,
            CanonicalEvidenceCollection::Approvals,
            2,
        );
        let repeated_request = approval(&fixture, 90, ApprovalSubject::FindingWaiver(low));
        assert_error(
            AcceptanceEvidence::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                vec![first, repeated_request],
                Vec::new(),
            )
            .unwrap_err(),
            EvidenceErrorKind::DuplicateObservation,
            CanonicalEvidenceCollection::Approvals,
            1,
        );
    }
}

#[test]
fn waiver_construction_allows_absent_approval_but_rejects_a_contradictory_subject() {
    let fixture = Fixture::new();
    let id = FindingId::new(bytes(50)).unwrap();
    let waiver = |request| {
        WaiverObservation::new(
            id,
            fixture.revision(),
            ApprovalRequestId::new(bytes(request)).unwrap(),
            fixture.waiver_authority,
            fixture.waiver_evidence,
            digest(93),
        )
    };
    let correct = approval(&fixture, 90, ApprovalSubject::FindingWaiver(id));
    let accepted = AcceptanceEvidence::new(
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![correct],
        vec![waiver(90)],
    )
    .unwrap();
    assert_eq!(accepted.waivers(), &[waiver(90)]);
    assert!(
        AcceptanceEvidence::new(Vec::new(), Vec::new(), Vec::new(), Vec::new(), vec![waiver(90)])
            .is_ok()
    );
    let wrong = approval(&fixture, 90, ApprovalSubject::Acceptance);
    assert_error(
        AcceptanceEvidence::new(Vec::new(), Vec::new(), Vec::new(), vec![wrong], vec![waiver(90)])
            .unwrap_err(),
        EvidenceErrorKind::WaiverApprovalSubjectMismatch,
        CanonicalEvidenceCollection::Waivers,
        0,
    );
    assert_error(
        AcceptanceEvidence::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![wrong],
            vec![waiver(89), waiver(90)],
        )
        .unwrap_err(),
        EvidenceErrorKind::DuplicateObservation,
        CanonicalEvidenceCollection::Waivers,
        1,
    );
    let gate = fixture.gate(fixture.revision(), GateOutcome::Passed);
    assert_error(
        AcceptanceEvidence::new(
            vec![gate, gate],
            Vec::new(),
            Vec::new(),
            vec![wrong],
            vec![waiver(90)],
        )
        .unwrap_err(),
        EvidenceErrorKind::DuplicateObservation,
        CanonicalEvidenceCollection::Gates,
        1,
    );
}
