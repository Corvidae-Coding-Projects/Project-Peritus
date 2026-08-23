use super::{MANIFEST, sha256_hex, valid_sha256};
use crate::error::Diagnostic;
use crate::trust::manifest_actor::ActorRegistry;
use crate::trust::manifest_file;
use crate::trust::manifest_model::{
    ProofImpactChange, ProofImpactDocument, ProofImpactEvidenceKind, ProofImpactFindingDisposition,
    ProofImpactGateResult, ProofImpactStatus, ProofImpactVerdict, ProofImpactVerdictDecision,
    ProofImpactVerdictFinding, ProofImpactVerdictGate,
};
use std::collections::BTreeSet;
use std::path::Path;

#[path = "verdict/artifact.rs"]
mod artifact;
#[path = "verdict/digest.rs"]
mod digest;
#[path = "verdict/directory.rs"]
mod directory;
#[path = "verdict/git.rs"]
mod git;
#[cfg(test)]
#[path = "verdict/tests.rs"]
mod tests;
#[path = "verdict/timestamp.rs"]
mod timestamp;

#[cfg(test)]
use directory::{ReviewPathKind, collect_review_files_with};

pub(super) const DIRECTORY: &str = "verification/reviews";
const SCHEMA: &str = "peritus.verification.proof-impact-verdict";

pub(super) fn validate_directory(
    root: &Path,
    document: &ProofImpactDocument,
    diagnostics: &mut Vec<Diagnostic>,
) {
    directory::validate(root, document, diagnostics);
}

pub(super) fn validate_change(
    root: &Path,
    actors: &ActorRegistry<'_>,
    change: &ProofImpactChange,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let required = change_number(&change.id).is_some_and(|number| number >= 5);
    let Some(reference) = &change.verdict else {
        if required {
            diagnostics.push(Diagnostic::at(
                MANIFEST,
                format!("post-genesis change `{}` has no detached reviewer verdict", change.id),
                "reference the exact content-addressed verification/reviews/PCR-NNNN.toml artifact",
            ));
        }
        return;
    };
    if !required {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!(
                "genesis change `{}` unexpectedly changes its historical verdict shape",
                change.id
            ),
            "retain PCR-0001 through PCR-0004 exactly as protected A1 history",
        ));
        return;
    }
    let expected_path = format!("{DIRECTORY}/{}.toml", change.id);
    if reference.path != expected_path || !valid_sha256(&reference.sha256) {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("change `{}` has a malformed detached-verdict reference", change.id),
            format!("reference `{expected_path}` and its exact lowercase raw-byte SHA-256"),
        ));
        return;
    }
    let path = Path::new(&reference.path);
    let Some((verdict, bytes)) =
        manifest_file::read_toml_with_bytes::<ProofImpactVerdict>(root, path, diagnostics)
    else {
        return;
    };
    if sha256_hex(&bytes) != reference.sha256 {
        diagnostics.push(Diagnostic::at(
            path,
            format!("detached verdict for `{}` does not match its PCR content address", change.id),
            "restore the reviewed raw bytes or update the future PCR before authorization lands",
        ));
    }
    validate_artifact(
        root,
        path,
        change,
        actors.principal(&change.reviewer),
        &verdict,
        diagnostics,
    );
}

pub(super) fn validate_new_authorization_bases(
    root: &Path,
    base_commit: &str,
    base: &ProofImpactDocument,
    current: &ProofImpactDocument,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(new_changes) = current.changes.get(base.changes.len()..) else { return };
    for change in new_changes {
        let Some(reference) = &change.verdict else { continue };
        let path = Path::new(&reference.path);
        let Some(verdict) = manifest_file::read_toml::<ProofImpactVerdict>(root, path, diagnostics)
        else {
            continue;
        };
        if verdict.authorization_base_commit != base_commit {
            diagnostics.push(Diagnostic::at(
                path,
                format!(
                    "new verdict `{}` is not bound to protected authorization base `{base_commit}`",
                    verdict.id
                ),
                "review the frozen implementation against the exact protected PR base commit",
            ));
        }
    }
}

fn validate_artifact(
    root: &Path,
    path: &Path,
    change: &ProofImpactChange,
    reviewer_principal: Option<&str>,
    verdict: &ProofImpactVerdict,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let envelope_matches = verdict.schema == SCHEMA
        && verdict.schema_version == 1
        && verdict.id == format!("VERDICT-{}", change.id)
        && verdict.pcr_id == change.id
        && verdict.reviewer == change.reviewer
        && reviewer_principal == Some(verdict.reviewer_principal.as_str())
        && timestamp::is_full_utc(&verdict.reviewed_at, &change.review_date);
    if !envelope_matches {
        diagnostics.push(Diagnostic::at(
            path,
            format!("detached verdict does not identify change `{}` and its reviewer", change.id),
            "bind the exact schema, PCR, registered actor/principal, and full UTC review timestamp",
        ));
    }
    if !full_commit(&verdict.authorization_base_commit)
        || !full_commit(&verdict.implementation_commit)
        || !full_commit(&verdict.implementation_tree)
        || verdict.implementation_commit == verdict.authorization_base_commit
    {
        diagnostics.push(Diagnostic::at(
            path,
            "detached verdict has malformed or aliased Git identities",
            "bind distinct nonzero full commit identities and the reviewed implementation tree",
        ));
    }
    git::validate(root, path, change, verdict, diagnostics);
    validate_digests(path, change, verdict, diagnostics);
    validate_gates(path, change, &verdict.gate_evidence, diagnostics);
    validate_findings(path, &verdict.findings, diagnostics);
    artifact::validate(root, path, verdict, diagnostics);
    if change.status == ProofImpactStatus::Approved
        && (verdict.decision != ProofImpactVerdictDecision::Approved
            || verdict
                .gate_evidence
                .iter()
                .any(|gate| gate.result != ProofImpactGateResult::Passed)
            || verdict.findings.iter().any(unresolved_blocker))
    {
        diagnostics.push(Diagnostic::at(
            path,
            format!("approved change `{}` lacks a gate-clean, blocker-free verdict", change.id),
            "reject the PCR or resolve every gate failure and blocking finding before authorization",
        ));
    }
}

fn validate_digests(
    path: &Path,
    change: &ProofImpactChange,
    verdict: &ProofImpactVerdict,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let actual = [
        digest::source_transitions(change),
        digest::gate_evidence(&verdict.gate_evidence),
        digest::findings(&verdict.findings),
    ];
    let declared = [
        &verdict.source_transitions_sha256,
        &verdict.gate_evidence_sha256,
        &verdict.finding_set_sha256,
    ];
    if declared.iter().any(|value| !valid_sha256(value))
        || declared.iter().zip(actual).any(|(declared, actual)| **declared != actual)
    {
        diagnostics.push(Diagnostic::at(
            path,
            "detached verdict digest does not match its transitions, gates, or findings",
            "recompute all three canonical SHA-256 bindings after the review record is final",
        ));
    }
}

fn validate_gates(
    path: &Path,
    change: &ProofImpactChange,
    gates: &[ProofImpactVerdictGate],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let canonical = gates.windows(2).all(|pair| gate_key(&pair[0]) < gate_key(&pair[1]));
    let expected: BTreeSet<_> = change
        .evidence
        .iter()
        .map(|item| (item.kind, item.owning_crate.as_str(), item.command.as_str()))
        .collect();
    let actual: BTreeSet<_> = gates.iter().map(gate_key).collect();
    if !canonical
        || gates.len() != actual.len()
        || actual != expected
        || gates.iter().any(|gate| !valid_sha256(&gate.output.sha256))
    {
        diagnostics.push(Diagnostic::at(
            path,
            format!("detached verdict gates do not exactly cover PCR evidence for `{}`", change.id),
            "record each canonical PCR evidence command once with its result and output digest",
        ));
    }
}

fn validate_findings(
    path: &Path,
    findings: &[ProofImpactVerdictFinding],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let canonical = findings.windows(2).all(|pair| pair[0].id < pair[1].id);
    let ids: BTreeSet<_> = findings.iter().map(|finding| finding.id.as_str()).collect();
    let valid = findings.iter().all(|finding| {
        valid_finding_id(&finding.id)
            && valid_sha256(&finding.detail.sha256)
            && valid_sha256(&finding.evidence.sha256)
    });
    if !canonical || ids.len() != findings.len() || !valid {
        diagnostics.push(Diagnostic::at(
            path,
            "detached verdict findings are malformed, duplicated, or noncanonical",
            "use sorted unique FINDING-NNNN IDs and exact detail/evidence content digests",
        ));
    }
}

const fn unresolved_blocker(finding: &ProofImpactVerdictFinding) -> bool {
    finding.blocking
        && !matches!(
            finding.disposition,
            ProofImpactFindingDisposition::Fixed
                | ProofImpactFindingDisposition::Invalid
                | ProofImpactFindingDisposition::Superseded
        )
}

const fn gate_key(gate: &ProofImpactVerdictGate) -> (ProofImpactEvidenceKind, &str, &str) {
    (gate.kind, gate.owning_crate.as_str(), gate.command.as_str())
}

fn change_number(id: &str) -> Option<u64> {
    id.strip_prefix("PCR-")?.parse().ok()
}

fn valid_finding_id(id: &str) -> bool {
    id.strip_prefix("FINDING-").is_some_and(|digits| {
        digits.len() == 4 && digits != "0000" && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn full_commit(value: &str) -> bool {
    value.len() == 40
        && value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        && value.bytes().any(|byte| byte != b'0')
}
