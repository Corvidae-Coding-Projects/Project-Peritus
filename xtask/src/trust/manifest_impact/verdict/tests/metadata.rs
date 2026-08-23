use super::*;
use std::io;

#[test]
fn exact_detached_verdict_is_accepted() {
    let fixture = GitFixture::new();
    let change = fixture_change();
    assert!(diagnostics(&fixture, &change, &verdict(&fixture, &change)).is_empty());
}

#[test]
fn identity_digest_gate_and_blocker_mutations_fail_closed() {
    let fixture = GitFixture::new();
    let change = fixture_change();

    let mut wrong_principal = verdict(&fixture, &change);
    wrong_principal.reviewer_principal = "aliased-principal".to_owned();
    assert!(
        diagnostics(&fixture, &change, &wrong_principal)
            .iter()
            .any(|item| item.message().contains("identify change"))
    );

    let mut wrong_digest = verdict(&fixture, &change);
    wrong_digest.source_transitions_sha256 = repeated('f', 64);
    assert!(
        diagnostics(&fixture, &change, &wrong_digest)
            .iter()
            .any(|item| item.message().contains("digest does not match"))
    );

    let mut failed_gate = verdict(&fixture, &change);
    failed_gate.gate_evidence[0].result = ProofImpactGateResult::Failed;
    failed_gate.gate_evidence_sha256 = digest::gate_evidence(&failed_gate.gate_evidence);
    assert!(
        diagnostics(&fixture, &change, &failed_gate)
            .iter()
            .any(|item| item.message().contains("gate-clean"))
    );

    let mut missing_gate = verdict(&fixture, &change);
    missing_gate.gate_evidence.clear();
    missing_gate.gate_evidence_sha256 = digest::gate_evidence(&missing_gate.gate_evidence);
    assert!(
        diagnostics(&fixture, &change, &missing_gate)
            .iter()
            .any(|item| item.message().contains("exactly cover"))
    );

    let mut blocker = verdict(&fixture, &change);
    blocker.findings[0].disposition = ProofImpactFindingDisposition::Open;
    blocker.finding_set_sha256 = digest::findings(&blocker.findings);
    assert!(
        diagnostics(&fixture, &change, &blocker)
            .iter()
            .any(|item| item.message().contains("blocker-free"))
    );
}

#[test]
fn retained_artifact_mutation_duplication_and_missing_report_fail_closed() {
    let fixture = GitFixture::new();
    let change = fixture_change();

    let mutated = verdict(&fixture, &change);
    fs::write(fixture.root.join(&mutated.gate_evidence[0].output.path), b"tampered\n")
        .expect("mutate gate output");
    assert!(
        diagnostics(&fixture, &change, &mutated)
            .iter()
            .any(|item| item.message().contains("differs from its content address"))
    );

    let mut duplicate = verdict(&fixture, &change);
    duplicate.artifacts.push(duplicate.artifacts[0].clone());
    duplicate.artifacts.sort();
    duplicate.artifact_inventory_sha256 = digest::artifact_inventory(&duplicate.artifacts);
    assert!(
        diagnostics(&fixture, &change, &duplicate)
            .iter()
            .any(|item| item.message().contains("one-to-one"))
    );

    let mut missing_report = verdict(&fixture, &change);
    missing_report
        .artifacts
        .retain(|artifact| artifact.kind != ProofImpactVerdictArtifactKind::ReviewReport);
    missing_report.artifact_inventory_sha256 =
        digest::artifact_inventory(&missing_report.artifacts);
    assert!(
        diagnostics(&fixture, &change, &missing_report)
            .iter()
            .any(|item| item.message().contains("one-to-one"))
    );
}

#[test]
fn canonical_digests_ignore_record_order_but_bind_every_field() {
    let fixture = GitFixture::new();
    let change = fixture_change();
    let value = verdict(&fixture, &change);
    let first = value.findings[0].clone();
    let mut second = first.clone();
    second.id = "FINDING-0002".to_owned();
    assert_eq!(
        digest::findings(&[first.clone(), second.clone()]),
        digest::findings(&[second.clone(), first.clone()])
    );
    let before = digest::findings(&[first.clone(), second.clone()]);
    second.evidence.sha256 = repeated('a', 64);
    assert_ne!(before, digest::findings(&[first, second]));

    let mut changed = fixture_change();
    changed.source_changes[0].source_file = "crate/src/other.rs".to_owned();
    assert_ne!(digest::source_transitions(&change), digest::source_transitions(&changed));
}

#[test]
fn verdict_schema_rejects_unknown_and_missing_fields() {
    let fixture = GitFixture::new();
    let change = fixture_change();
    let serialized =
        toml::to_string(&verdict(&fixture, &change)).expect("fixture verdict must serialize");
    assert!(
        toml::from_str::<ProofImpactVerdict>(&format!("{serialized}\nunknown = true\n")).is_err()
    );
    assert!(toml::from_str::<ProofImpactVerdict>("schema = 'incomplete'").is_err());
}

#[test]
fn recursive_review_directory_rejects_uninventoried_files() {
    let fixture = GitFixture::new();
    let mut change = fixture_change();
    let value = verdict(&fixture, &change);
    let bytes = toml::to_string(&value).expect("serialize verdict");
    let verdict_path = fixture.root.join("verification/reviews/PCR-0005.toml");
    fs::write(&verdict_path, &bytes).expect("write verdict");
    change.verdict.as_mut().expect("verdict reference").sha256 = sha256_hex(bytes.as_bytes());
    let document = ProofImpactDocument {
        schema: "peritus.verification.proof-impact".to_owned(),
        schema_version: 1,
        baseline: "A1".to_owned(),
        hash_algorithm: "sha256-raw-bytes-v1".to_owned(),
        sources: Vec::new(),
        changes: vec![change],
    };
    let mut diagnostics = Vec::new();
    validate_directory(&fixture.root, &document, &mut diagnostics);
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");

    let extra = fixture.root.join("verification/reviews/PCR-0005/gates/uninventoried.txt");
    fs::write(extra, b"not in the verdict inventory\n").expect("write extra artifact");
    validate_directory(&fixture.root, &document, &mut diagnostics);
    assert!(
        diagnostics.iter().any(|item| item.message().contains("not referenced by exactly one"))
    );
}

#[test]
fn unreadable_directory_entries_and_nested_directories_fail_closed() {
    let root = Path::new("/repository");
    let directory = root.join("verification/reviews");
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    collect_review_files_with(
        root,
        &directory,
        true,
        &mut files,
        &mut diagnostics,
        &|_| Ok(vec![Err(io::Error::new(io::ErrorKind::PermissionDenied, "entry denied"))]),
        &|_| Ok(ReviewPathKind::File),
    );
    assert!(diagnostics.iter().any(|item| item.message().contains("entry denied")));

    diagnostics.clear();
    collect_review_files_with(
        root,
        &directory,
        true,
        &mut files,
        &mut diagnostics,
        &|path| {
            if path == directory {
                Ok(vec![Ok(path.join("PCR-0005"))])
            } else {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "nested denied"))
            }
        },
        &|_| Ok(ReviewPathKind::Directory),
    );
    assert!(diagnostics.iter().any(|item| item.message().contains("nested denied")));
}
