//! Authorization checks the proposed complete candidate while leaving protected source untouched.
//!
//! This phase is derived from the exact unchanged inventory and one appended review record;
//! there is no command-line bypass, approved hash exception, or reduced implementation gate.

use super::{
    MANIFEST, ManifestContext, candidate_actors::CandidateActors, candidate_inventory,
    candidate_tree, review_base,
};
use crate::error::{Diagnostic, XtaskError};
use crate::trust::manifest_actor_model::ActorsDocument;
use crate::trust::manifest_file;
use crate::trust::manifest_model::{ProofImpactDocument, ProofImpactVerdict};
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn is_phase(
    context: &ManifestContext<'_>,
    current: &ProofImpactDocument,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<bool, XtaskError> {
    Ok(load(context, current, diagnostics)?.is_some())
}

pub(super) fn validate(
    context: &ManifestContext<'_>,
    current: &ProofImpactDocument,
    enforce_review_base: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<bool, XtaskError> {
    let Some(phase) = load(context, current, diagnostics)? else { return Ok(false) };
    let AuthorizationPhase { change, verdict, base } = phase;
    if !one_complete_authorization(&base, current) {
        diagnostics.push(error("authorization requires exactly one appended immutable review"));
        return Ok(true);
    }
    validate_baseline_tree(context.root, &verdict.authorization_base_commit, diagnostics)?;
    super::checker_binding::validate(context.root, &verdict.implementation_tree, diagnostics)?;
    let Some(protected_actors) = manifest_file::read_toml::<ActorsDocument>(
        context.root,
        Path::new("verification/actors.toml"),
        diagnostics,
    ) else {
        return Ok(true);
    };
    let Some(candidate) =
        CandidateActors::load(context.root, &protected_actors, change, &verdict, diagnostics)
    else {
        return Ok(true);
    };
    let registry = candidate.registry(diagnostics);
    super::validate_changes(context, &registry, current, diagnostics);
    super::verdict::validate_directory(context.root, current, diagnostics);
    candidate_inventory::validate(
        context.root,
        &verdict.implementation_tree,
        &base,
        current,
        change,
        diagnostics,
    )?;
    if enforce_review_base {
        review_base::validate(context.root, current, diagnostics)?;
    }
    Ok(true)
}

struct AuthorizationPhase<'a> {
    change: &'a super::ProofImpactChange,
    verdict: ProofImpactVerdict,
    base: ProofImpactDocument,
}

fn load<'a>(
    context: &ManifestContext<'_>,
    current: &'a ProofImpactDocument,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<AuthorizationPhase<'a>>, XtaskError> {
    let Some(change) = current.changes.last() else { return Ok(None) };
    let Some(reference) = &change.verdict else { return Ok(None) };
    let Some(verdict) = manifest_file::read_toml::<ProofImpactVerdict>(
        context.root,
        Path::new(&reference.path),
        diagnostics,
    ) else {
        return Ok(None);
    };
    if !review_base::full_commit(&verdict.authorization_base_commit) {
        return Ok(None);
    }
    let Some(base) = review_base::load_base_manifest(
        context.root,
        &verdict.authorization_base_commit,
        diagnostics,
    )?
    else {
        return Ok(None);
    };
    if current.sources != base.sources {
        return Ok(None);
    }
    Ok(Some(AuthorizationPhase { change, verdict, base }))
}

fn validate_baseline_tree(
    root: &Path,
    base: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let changed = candidate_tree::git(root, &["diff", "--name-only", "-z", base, "--"])?;
    let untracked =
        candidate_tree::git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let mut paths = BTreeSet::new();
    for bytes in [&changed, &untracked] {
        for path in bytes.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
            let path = std::str::from_utf8(path)
                .map_err(|_| XtaskError::metadata("authorization has a non-UTF-8 changed path"))?;
            paths.insert(path.to_owned());
        }
    }
    for path in paths.iter().filter(|path| !authorization_path(path)) {
        diagnostics.push(Diagnostic::at(path,
            "authorization changes protected application, policy, or formal source bytes",
            "land only checker/review artifacts in the authorization phase; apply the exact reviewed source afterward"));
    }
    Ok(())
}

fn authorization_path(path: &str) -> bool {
    path.starts_with("xtask/src/")
        || path.starts_with("xtask/tests/")
        || path == "xtask/README.md"
        || path == "verification/README.md"
        || path == MANIFEST
        || path.starts_with("verification/reviews/")
        || (path.starts_with(".design/")
            && Path::new(path).extension().is_some_and(|extension| extension == "md"))
}

fn one_complete_authorization(base: &ProofImpactDocument, current: &ProofImpactDocument) -> bool {
    current.changes.len() == base.changes.len() + 1
        && current.changes.get(..base.changes.len()) == Some(base.changes.as_slice())
        && candidate_inventory::history_is_applied(base)
}

fn error(message: &str) -> Diagnostic {
    Diagnostic::at(
        MANIFEST,
        message,
        "preserve the protected inventory/history and append one complete exact-candidate authorization",
    )
}

#[cfg(test)]
mod tests {
    use super::{authorization_path, is_phase, one_complete_authorization};
    use crate::trust::manifest_context::ManifestContext;
    use crate::trust::manifest_model::{
        ProofImpactChange, ProofImpactKind, ProofImpactPackage, ProofImpactSnapshot,
        ProofImpactSource, ProofImpactStatus, ProofImpactVerdict, ProofImpactVerdictArtifactRef,
        ProofImpactVerdictDecision, ProofImpactVerdictRef, ProofSourceChange,
    };
    use crate::trust::manifest_tests::{Fixture, cargo, policy};
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    #[test]
    fn authorization_cannot_change_application_or_formal_policy() {
        for path in [
            "Cargo.toml",
            "Cargo.lock",
            "architecture.toml",
            "verification/actors.toml",
            "verification/actor-provenance.json",
            "verification/trust.toml",
            ".cargo/config.toml",
            "crates/app/peritus-tui/src/lib.rs",
            ".github/workflows/verification.yml",
        ] {
            assert!(!authorization_path(path), "{path}");
        }
        for path in [
            "xtask/src/trust.rs",
            "xtask/tests/trust.rs",
            "verification/proof-impact.toml",
            "verification/reviews/PCR-0005.toml",
            ".design/formal-review.md",
        ] {
            assert!(authorization_path(path), "{path}");
        }
    }

    #[test]
    fn unchanged_protected_inventory_selects_the_authorization_phase() {
        let fixture = Fixture::new();
        fixture.write(
            "verification/proof-impact.toml",
            "schema = 'peritus.verification.proof-impact'\nschema_version = 1\nbaseline = 'A1'\nhash_algorithm = 'sha256-raw-bytes-v1'\nsources = []\nchanges = []\n",
        );
        git(fixture.path(), &["init", "--quiet"]);
        git(fixture.path(), &["config", "user.name", "Peritus Test"]);
        git(fixture.path(), &["config", "user.email", "peritus-test@example.invalid"]);
        git(fixture.path(), &["config", "commit.gpgsign", "false"]);
        git(fixture.path(), &["add", "verification/proof-impact.toml"]);
        git(fixture.path(), &["commit", "--quiet", "-m", "protected"]);
        let base = stdout(fixture.path(), &["rev-parse", "HEAD"]);

        let mut current: crate::trust::manifest_model::ProofImpactDocument = toml::from_str(
            &fs::read_to_string(fixture.path().join("verification/proof-impact.toml"))
                .expect("protected manifest"),
        )
        .expect("protected manifest schema");
        let verdict_path = "verification/reviews/PCR-0005.toml";
        current.changes.push(authorization_change(verdict_path));
        let verdict = authorization_verdict(&base);
        fixture.write(verdict_path, &toml::to_string(&verdict).expect("serialize verdict fixture"));
        assert!(one_complete_authorization(
            &toml::from_str(
                &fs::read_to_string(fixture.path().join("verification/proof-impact.toml"))
                    .expect("protected manifest")
            )
            .expect("protected manifest schema"),
            &current,
        ));

        let policy = policy();
        let cargo = cargo(&fixture);
        let context = ManifestContext::new(fixture.path(), &policy, &cargo);
        let mut diagnostics = Vec::new();
        assert!(is_phase(&context, &current, &mut diagnostics).expect("classify authorization"));
        assert!(diagnostics.is_empty(), "unexpected classification diagnostics: {diagnostics:?}");

        current.sources.push(ProofImpactSource {
            source_file: "crate/src/lib.rs".to_owned(),
            sha256: "a".repeat(64),
            affected_packages: vec![ProofImpactPackage {
                package: "crate".to_owned(),
                verification_class: "V".to_owned(),
            }],
            change_id: "PCR-0005".to_owned(),
        });
        assert!(!is_phase(&context, &current, &mut diagnostics).expect("reject implementation"));
    }

    fn authorization_change(verdict_path: &str) -> ProofImpactChange {
        ProofImpactChange {
            id: "PCR-0005".to_owned(),
            status: ProofImpactStatus::Approved,
            change_kinds: vec![
                ProofImpactKind::Executable,
                ProofImpactKind::Specification,
                ProofImpactKind::Precondition,
                ProofImpactKind::Postcondition,
                ProofImpactKind::Proof,
            ],
            source_changes: vec![ProofSourceChange {
                source_file: "crate/src/lib.rs".to_owned(),
                previous: None,
                current: Some(ProofImpactSnapshot {
                    sha256: "a".repeat(64),
                    affected_packages: vec![ProofImpactPackage {
                        package: "crate".to_owned(),
                        verification_class: "V".to_owned(),
                    }],
                }),
            }],
            rationale: "authorize exact frozen candidate".to_owned(),
            impact: "records complete independent review".to_owned(),
            evidence: Vec::new(),
            owner: "ACTOR-0001".to_owned(),
            reviewer: "ACTOR-0002".to_owned(),
            review_date: "2026-09-13".to_owned(),
            verdict: Some(ProofImpactVerdictRef {
                path: verdict_path.to_owned(),
                sha256: "b".repeat(64),
            }),
        }
    }

    fn authorization_verdict(base: &str) -> ProofImpactVerdict {
        let empty = ProofImpactVerdictArtifactRef { path: String::new(), sha256: String::new() };
        ProofImpactVerdict {
            schema: "peritus.verification.proof-impact-verdict".to_owned(),
            schema_version: 1,
            id: "VERDICT-PCR-0005".to_owned(),
            pcr_id: "PCR-0005".to_owned(),
            reviewer: "ACTOR-0002".to_owned(),
            reviewer_principal: "fixture-reviewer".to_owned(),
            authorization_base_commit: base.to_owned(),
            implementation_commit: "1".repeat(40),
            implementation_tree: "2".repeat(40),
            source_transitions_sha256: "3".repeat(64),
            gate_evidence_sha256: "4".repeat(64),
            finding_set_sha256: "5".repeat(64),
            artifact_inventory_sha256: "6".repeat(64),
            decision: ProofImpactVerdictDecision::Approved,
            reviewed_at: "2026-09-13T00:00:00Z".to_owned(),
            review_report: empty,
            gate_evidence: Vec::new(),
            findings: Vec::new(),
            artifacts: Vec::new(),
        }
    }

    fn git(root: &Path, arguments: &[&str]) {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(root)
            .output()
            .expect("Git fixture command");
        assert!(output.status.success(), "Git failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    fn stdout(root: &Path, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(root)
            .output()
            .expect("Git fixture command");
        assert!(output.status.success(), "Git failed: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).expect("Git output UTF-8").trim().to_owned()
    }
}
