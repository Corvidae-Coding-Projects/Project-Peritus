//! Recompute a proposed inventory from the frozen tree using the normal discovery boundary.

use super::{ManifestContext, candidate_tree::CandidateTree, inventory, validate_source_inventory};
use crate::error::{Diagnostic, XtaskError};
use crate::trust::manifest_model::{ProofImpactChange, ProofImpactDocument, ProofImpactSource};
use std::collections::BTreeMap;
use std::path::Path;

pub(super) fn validate(
    repository: &Path,
    tree: &str,
    base: &ProofImpactDocument,
    current: &ProofImpactDocument,
    change: &ProofImpactChange,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let candidate = CandidateTree::materialize(repository, tree)?;
    let root = candidate.root();
    let policy = crate::metadata::architecture_policy(root)?;
    crate::cli::all::check_candidate(root)?;
    let cargo = crate::metadata::cargo_metadata(root)?;
    let (target_roots, target_diagnostics) = crate::trust::workspace_target_policy(root, &cargo);
    diagnostics.extend(target_diagnostics);
    let discovery = crate::source::discover_compilation_sources(root, &policy, &target_roots)?;
    diagnostics.extend(discovery.diagnostics);
    let context = ManifestContext::new(root, &policy, &cargo);
    let expected = inventory::expected_sources(&context, &discovery.files);
    let mut projected = current.clone();
    projected.sources = overlay(base, change);
    let changes = current.changes.iter().map(|item| (item.id.as_str(), item)).collect();
    validate_source_inventory(&context, &projected, &expected, &changes, diagnostics)
}

#[cfg(test)]
fn validate_local_trust(
    root: &Path,
    policy: &crate::model::ArchitecturePolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    match crate::trust::check_candidate(root, policy) {
        Ok(_) => Ok(()),
        Err(error) if !error.diagnostics().is_empty() => {
            diagnostics.extend(error.diagnostics().iter().cloned());
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn overlay(base: &ProofImpactDocument, change: &ProofImpactChange) -> Vec<ProofImpactSource> {
    let mut sources: BTreeMap<_, _> =
        base.sources.iter().map(|source| (source.source_file.clone(), source.clone())).collect();
    for transition in &change.source_changes {
        if let Some(snapshot) = &transition.current {
            sources.insert(
                transition.source_file.clone(),
                ProofImpactSource {
                    source_file: transition.source_file.clone(),
                    sha256: snapshot.sha256.clone(),
                    affected_packages: snapshot.affected_packages.clone(),
                    change_id: change.id.clone(),
                },
            );
        } else {
            sources.remove(&transition.source_file);
        }
    }
    sources.into_values().collect()
}

pub(super) fn history_is_applied(document: &ProofImpactDocument) -> bool {
    let mut applied = document.clone();
    applied.sources.clear();
    for change in &document.changes {
        applied.sources = overlay(&applied, change);
    }
    let mut declared = document.sources.clone();
    declared.sort_by(|left, right| left.source_file.cmp(&right.source_file));
    applied.sources == declared
}

#[cfg(test)]
mod tests {
    use super::{history_is_applied, overlay, validate_local_trust};
    use crate::trust::manifest_model::{
        ProofImpactChange, ProofImpactDocument, ProofImpactPackage, ProofImpactSnapshot,
        ProofImpactSource, ProofImpactStatus, ProofSourceChange,
    };
    use crate::trust::manifest_tests::{
        Fixture, excluded_obligation, make_cargo_metadata_runnable, policy, trust_entry,
        write_coverage_documents, write_fixture,
    };

    fn baseline() -> ProofImpactDocument {
        let source = ProofImpactSource {
            source_file: "crate/src/lib.rs".to_owned(),
            sha256: "a".repeat(64),
            affected_packages: vec![ProofImpactPackage {
                package: "crate".to_owned(),
                verification_class: "V".to_owned(),
            }],
            change_id: "PCR-0001".to_owned(),
        };
        ProofImpactDocument {
            schema: "peritus.verification.proof-impact".to_owned(),
            schema_version: 1,
            baseline: "A1".to_owned(),
            hash_algorithm: "sha256-raw-bytes-v1".to_owned(),
            sources: vec![source.clone()],
            changes: vec![ProofImpactChange {
                id: source.change_id,
                status: ProofImpactStatus::Approved,
                change_kinds: vec![],
                source_changes: vec![ProofSourceChange {
                    source_file: source.source_file,
                    previous: None,
                    current: Some(ProofImpactSnapshot {
                        sha256: source.sha256,
                        affected_packages: source.affected_packages,
                    }),
                }],
                rationale: "fixture transition".to_owned(),
                impact: "fixture impact".to_owned(),
                evidence: vec![],
                owner: "owner".to_owned(),
                reviewer: "reviewer".to_owned(),
                review_date: "2026-09-09".to_owned(),
                verdict: None,
            }],
        }
    }

    #[test]
    fn pending_authorization_cannot_be_stacked_or_mistaken_for_applied_history() {
        let base = baseline();
        assert!(history_is_applied(&base));
        let mut pending = base.clone();
        let mut change = base.changes.last().expect("baseline history").clone();
        change.id = "PCR-9000".to_owned();
        change.source_changes = vec![ProofSourceChange {
            source_file: "new/src/lib.rs".to_owned(),
            previous: None,
            current: Some(ProofImpactSnapshot {
                sha256: "c".repeat(64),
                affected_packages: base.sources[0].affected_packages.clone(),
            }),
        }];
        pending.changes.push(change.clone());
        assert!(!history_is_applied(&pending));
        pending.sources = overlay(&base, &change);
        assert!(history_is_applied(&pending));
    }

    #[test]
    fn overlay_preserves_unchanged_identity_and_applies_exact_removals() {
        let base = baseline();
        let mut change = base.changes.last().expect("baseline history").clone();
        change.source_changes = vec![ProofSourceChange {
            source_file: base.sources[0].source_file.clone(),
            previous: Some(ProofImpactSnapshot {
                sha256: base.sources[0].sha256.clone(),
                affected_packages: base.sources[0].affected_packages.clone(),
            }),
            current: None,
        }];
        let result = overlay(&base, &change);
        assert_eq!(result.len(), base.sources.len() - 1);
        assert!(result.iter().all(|source| base.sources.contains(source)));
        assert!(!result.iter().any(|source| source.source_file == base.sources[0].source_file));
    }

    #[test]
    fn materialized_candidate_general_trust_accepts_a_complete_candidate() {
        let fixture = candidate_fixture();
        let mut diagnostics = Vec::new();
        validate_local_trust(fixture.path(), &policy(), &mut diagnostics)
            .expect("complete candidate trust fixture must be executable");
        assert!(diagnostics.is_empty(), "unexpected candidate diagnostics: {diagnostics:?}");
    }

    #[test]
    fn materialized_candidate_rejects_an_invalid_obligation() {
        let fixture = candidate_fixture();
        write_coverage_documents(&fixture, "[]", excluded_obligation());
        let mut diagnostics = Vec::new();
        validate_local_trust(fixture.path(), &policy(), &mut diagnostics)
            .expect("invalid candidate obligation must produce diagnostics");
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.path() == Some(std::path::Path::new("verification/obligations.toml"))
                    && diagnostic.message().contains("matching live exclusion")
            }),
            "missing candidate obligation diagnostic: {diagnostics:?}"
        );
    }

    #[test]
    fn materialized_candidate_rejects_trusted_construct_outside_the_trust_root() {
        let fixture = candidate_fixture();
        fixture.write(
            "crates/foundation/peritus-types/src/lib.rs",
            "pub fn value() -> u64 { assume(false); 1 }\n",
        );
        let mut diagnostics = Vec::new();
        validate_local_trust(fixture.path(), &policy(), &mut diagnostics)
            .expect("invalid candidate trusted construct must produce diagnostics");
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.path()
                    == Some(std::path::Path::new("crates/foundation/peritus-types/src/lib.rs"))
                    && diagnostic.message().contains("outside an allowed trust root")
            }),
            "missing candidate trusted-construct diagnostic: {diagnostics:?}"
        );
    }

    fn candidate_fixture() -> Fixture {
        let fixture = Fixture::new();
        write_fixture(&fixture, trust_entry());
        make_cargo_metadata_runnable(&fixture);
        let proof_impact =
            std::fs::read_to_string(fixture.path().join("verification/proof-impact.toml"))
                .expect("fixture proof-impact manifest");
        fixture.write(
            "verification/proof-impact.toml",
            &format!(
                r#"{proof_impact}
[[changes]]
id = "PCR-0005"
status = "approved"
change_kinds = ["executable", "specification", "precondition", "postcondition", "proof"]
rationale = "legacy authorization reference retained in candidate"
impact = "candidate-local trust must not recursively classify this record"
owner = "ACTOR-0001"
reviewer = "ACTOR-0002"
review_date = "2026-09-13"
source_changes = []
evidence = []
verdict = {{ path = "verification/reviews/PCR-0005.toml", sha256 = "{}" }}
"#,
                "a".repeat(64),
            ),
        );
        fixture
    }
}
