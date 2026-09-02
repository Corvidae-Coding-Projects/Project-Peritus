//! Independent H0 review admission and contradiction checks.

use peritus_security_qualification::{
    IntegratedCandidate, ReviewCompletion, candidate_json, parse_review_json,
};
use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, Sha256Digest, WorkspaceId,
};
use serde_json::{Value, json};

#[test]
fn complete_independent_review_parses_for_the_exact_candidate() {
    let document = review_document(true, &[]);
    let review = parse_review_json(&serde_json::to_vec(&document).expect("review JSON"))
        .expect("valid external review");

    assert_eq!(review.candidate(), integrated_candidate());
    assert_eq!(review.completion(), ReviewCompletion::Completed);
    assert!(review.independent_from_producer());
    assert_eq!(review.scopes().len(), 5);
    assert!(review.findings().is_empty());
}

#[test]
fn claimed_independence_must_match_the_review_identities() {
    let document = review_document(false, &[]);
    let error = parse_review_json(&serde_json::to_vec(&document).expect("review JSON"))
        .expect_err("contradictory independence must fail");

    assert!(error.detail().contains("independence claim contradicts"));
}

#[test]
fn finding_lifecycle_requires_its_exact_evidence_shape() {
    let finding = json!({
        "finding_id": repeated_hex(7, 16),
        "candidate_source_sha256": repeated_hex(6, 32),
        "severity": "high",
        "lifecycle": "resolved",
        "authority_sha256": null,
        "remediation_sha256": repeated_hex(8, 32),
        "retest_sha256": null
    });
    let document = review_document(true, &[finding]);
    let error = parse_review_json(&serde_json::to_vec(&document).expect("review JSON"))
        .expect_err("incomplete resolution must fail");

    assert!(error.detail().contains("finding lifecycle"));
}

fn review_document(independent: bool, findings: &[Value]) -> Value {
    let envelope: Value =
        serde_json::from_slice(&candidate_json(integrated_candidate()).expect("candidate JSON"))
            .expect("candidate envelope");
    json!({
        "candidate": envelope.get("candidate").expect("candidate object").clone(),
        "reviewer_actor": repeated_hex(1, 16),
        "reviewer_organization_sha256": repeated_hex(2, 32),
        "review_context_sha256": repeated_hex(3, 32),
        "producer_actor": repeated_hex(4, 16),
        "producer_organization_sha256": repeated_hex(5, 32),
        "completion": "completed",
        "scopes": [
            "sandbox-escape",
            "authority-isolation",
            "evolution-and-promotion",
            "supply-chain",
            "unsafe-and-tcb"
        ],
        "independent_from_producer": independent,
        "report_sha256": repeated_hex(9, 32),
        "findings": findings
    })
}

fn repeated_hex(byte: u8, bytes: usize) -> String {
    format!("{byte:02x}").repeat(bytes)
}

fn integrated_candidate() -> IntegratedCandidate {
    IntegratedCandidate::new(
        RevisionTuple::new(
            AcceptanceSpecId::new([1; 16]).expect("acceptance"),
            HarnessId::new([2; 16]).expect("harness"),
            WorkspaceId::new([3; 16]).expect("workspace"),
            Generation::first(),
            RevisionNumber::first(),
            PolicyId::new([4; 16]).expect("policy"),
            ProviderProfileId::new([5; 16]).expect("provider"),
        ),
        Sha256Digest::new([6; 32]),
        Sha256Digest::new([7; 32]),
        Sha256Digest::new([8; 32]),
    )
}
