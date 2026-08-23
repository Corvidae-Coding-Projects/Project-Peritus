use super::{
    classify, full_commit, pre_authorized, same_identity, transition_for,
    validate_immutable_history,
};
use crate::trust::manifest_model::{ProofImpactDocument, ProofImpactKind, ProofImpactPackage};

#[test]
fn commit_identity_is_exact_and_nonzero() {
    assert!(full_commit("92f466f247f45128c630d1c843fd6e27d2115587"));
    assert!(!full_commit("92f466f"));
    assert!(!full_commit("0000000000000000000000000000000000000000"));
}

#[test]
fn contract_and_proof_changes_cannot_be_labeled_only_as_proof() {
    let kinds = classify(
        b"fn f() ensures result > 0 { proof { assert(true); } }",
        b"fn f() ensures result >= 0 { proof { assert(false); } }",
    );
    assert!(kinds.contains(&ProofImpactKind::Postcondition));
    assert!(kinds.contains(&ProofImpactKind::Specification));
    assert!(kinds.contains(&ProofImpactKind::Proof));
    assert!(kinds.contains(&ProofImpactKind::Executable));
}

#[test]
fn deep_multiline_contract_change_is_never_missed() {
    let padding = "\nvalue == value".repeat(16);
    let old = format!("fn f() ensures{padding}\nvalue > 0 {{ 1 }}");
    let new = format!("fn f() ensures{padding}\nvalue >= 0 {{ 1 }}");
    let kinds = classify(old.as_bytes(), new.as_bytes());
    assert_eq!(kinds.len(), 5);
    assert!(kinds.contains(&ProofImpactKind::Postcondition));
}

#[test]
fn rebaselining_and_same_change_approval_are_not_authorizations() {
    let old = "a".repeat(64);
    let new = "b".repeat(64);
    let base = document(&old, "PCR-0001", None);
    let rebased = document(&new, "PCR-0002", None);
    assert!(
        transition_for(
            "crate/src/lib.rs",
            Some(&base.sources[0]),
            Some(&rebased.sources[0]),
            &rebased,
        )
        .is_none()
    );

    let chained = document(&new, "PCR-0002", Some(&old));
    let candidate = transition_for(
        "crate/src/lib.rs",
        Some(&base.sources[0]),
        Some(&chained.sources[0]),
        &chained,
    )
    .expect("exact chain is present in current document");
    assert!(!pre_authorized(candidate, &base));
}

#[test]
fn protected_history_cannot_be_deleted() {
    let base = document(&"a".repeat(64), "PCR-0001", None);
    let mut current = base.clone();
    current.changes.clear();
    let mut diagnostics = Vec::new();
    validate_immutable_history(&base, &current, &mut diagnostics);
    assert!(diagnostics.iter().any(|item| item.message().contains("immutable prefix")));
}

#[test]
fn protected_history_cannot_be_reordered_or_prepended() {
    let mut base = document(&"a".repeat(64), "PCR-0001", None);
    let mut second = base.changes[0].clone();
    second.id = "PCR-0002".to_owned();
    base.changes.push(second);

    let mut reordered = base.clone();
    reordered.changes.swap(0, 1);
    let mut prepended = base.clone();
    let mut inserted = base.changes[0].clone();
    inserted.id = "PCR-0003".to_owned();
    prepended.changes.insert(0, inserted);

    for altered in [reordered, prepended] {
        let mut diagnostics = Vec::new();
        validate_immutable_history(&base, &altered, &mut diagnostics);
        assert!(diagnostics.iter().any(|item| item.message().contains("immutable prefix")));
    }
}

#[test]
fn affected_scope_is_part_of_the_protected_identity() {
    let before = document(&"a".repeat(64), "PCR-0001", None);
    let mut after = before.clone();
    after.sources[0].affected_packages.push(ProofImpactPackage {
        package: "other-formal-package".to_owned(),
        verification_class: "T".to_owned(),
    });
    assert!(!same_identity(Some(&before.sources[0]), Some(&after.sources[0])));
}

fn document(hash: &str, id: &str, previous: Option<&str>) -> ProofImpactDocument {
    let previous = previous.map_or_else(String::new, |value| {
        format!(
            "previous = {{ sha256 = \"{value}\", affected_packages = [{{ package = \"crate\", verification_class = \"V\" }}] }},"
        )
    });
    toml::from_str(&format!(
        r#"schema = "peritus.verification.proof-impact"
schema_version = 1
baseline = "A1"
hash_algorithm = "sha256-raw-bytes-v1"
sources = [{{ source_file = "crate/src/lib.rs", sha256 = "{hash}", affected_packages = [{{ package = "crate", verification_class = "V" }}], change_id = "{id}" }}]
changes = [{{ id = "{id}", status = "approved", change_kinds = ["executable", "specification", "precondition", "postcondition", "proof"], source_changes = [{{ source_file = "crate/src/lib.rs", {previous} current = {{ sha256 = "{hash}", affected_packages = [{{ package = "crate", verification_class = "V" }}] }} }}], rationale = "reviewed exact fixture transition", impact = "fixture impact remains explicit", evidence = [], owner = "fixture-owner", reviewer = "fixture-reviewer", review_date = "2026-08-21" }}]
"#
    ))
    .expect("proof-impact fixture must parse")
}
