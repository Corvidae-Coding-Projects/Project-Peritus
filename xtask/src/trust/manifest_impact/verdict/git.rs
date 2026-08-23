//! Exact Git object, ancestry, tree, and reviewed-snapshot bindings.

use super::sha256_hex;
use crate::error::Diagnostic;
use crate::trust::manifest_file;
use crate::trust::manifest_model::{ProofImpactChange, ProofImpactVerdict};
use std::path::Path;
use std::process::{Command, Output};

pub(super) fn validate(
    root: &Path,
    verdict_path: &Path,
    change: &ProofImpactChange,
    verdict: &ProofImpactVerdict,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !object_has_type(
        root,
        verdict_path,
        &verdict.authorization_base_commit,
        "commit",
        diagnostics,
    ) || !object_has_type(
        root,
        verdict_path,
        &verdict.implementation_commit,
        "commit",
        diagnostics,
    ) || !object_has_type(root, verdict_path, &verdict.implementation_tree, "tree", diagnostics)
    {
        return;
    }
    validate_tree_identity(root, verdict_path, verdict, diagnostics);
    validate_ancestry(root, verdict_path, verdict, diagnostics);
    validate_snapshots(root, verdict_path, change, verdict, diagnostics);
}

fn object_has_type(
    root: &Path,
    verdict_path: &Path,
    object: &str,
    expected: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let Some(output) = git(root, &["cat-file", "-t", object], verdict_path, diagnostics) else {
        return false;
    };
    let actual = output.status.success().then(|| String::from_utf8_lossy(&output.stdout));
    if actual.as_deref().map(str::trim) != Some(expected) {
        diagnostics.push(Diagnostic::at(
            verdict_path,
            format!("detached verdict Git object `{object}` is not an available {expected}"),
            "fetch the reviewed history and bind the exact full object ID of the required kind",
        ));
        return false;
    }
    true
}

fn validate_tree_identity(
    root: &Path,
    verdict_path: &Path,
    verdict: &ProofImpactVerdict,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let expression = format!("{}^{{tree}}", verdict.implementation_commit);
    let Some(output) =
        git(root, &["rev-parse", "--verify", &expression], verdict_path, diagnostics)
    else {
        return;
    };
    let actual = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || actual.trim() != verdict.implementation_tree {
        diagnostics.push(Diagnostic::at(
            verdict_path,
            "detached verdict implementation tree is not the exact tree of its implementation commit",
            "record the full `implementation_commit^{tree}` object ID without substituting another tree",
        ));
    }
}

fn validate_ancestry(
    root: &Path,
    verdict_path: &Path,
    verdict: &ProofImpactVerdict,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(output) = git(
        root,
        &[
            "merge-base",
            "--is-ancestor",
            &verdict.authorization_base_commit,
            &verdict.implementation_commit,
        ],
        verdict_path,
        diagnostics,
    ) else {
        return;
    };
    if !output.status.success() {
        diagnostics.push(Diagnostic::at(
            verdict_path,
            "detached verdict implementation commit does not descend from its authorization base",
            "review an implementation commit built on the exact protected authorization base",
        ));
    }
}

fn validate_snapshots(
    root: &Path,
    verdict_path: &Path,
    change: &ProofImpactChange,
    verdict: &ProofImpactVerdict,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for source in &change.source_changes {
        let relative = Path::new(&source.source_file);
        if !manifest_file::repository_relative(relative) {
            diagnostics.push(Diagnostic::at(
                verdict_path,
                format!("reviewed source path `{}` is not repository-relative", source.source_file),
                "bind only normalized repository-owned formal source paths",
            ));
            continue;
        }
        let object = format!("{}:{}", verdict.implementation_tree, source.source_file);
        let Some(output) = git(root, &["cat-file", "blob", &object], verdict_path, diagnostics)
        else {
            continue;
        };
        match &source.current {
            Some(snapshot)
                if !output.status.success() || sha256_hex(&output.stdout) != snapshot.sha256 =>
            {
                diagnostics.push(Diagnostic::at(
                    verdict_path,
                    format!(
                        "reviewed current snapshot for `{}` does not match the implementation tree",
                        source.source_file
                    ),
                    "bind the exact implementation tree whose raw source bytes match every PCR current digest",
                ));
            }
            None if output.status.success() => diagnostics.push(Diagnostic::at(
                verdict_path,
                format!(
                    "PCR removes `{}` but the reviewed implementation tree still contains it",
                    source.source_file
                ),
                "bind an implementation tree in which the reviewed source removal is exact",
            )),
            Some(_) | None => {}
        }
    }
}

fn git(
    root: &Path,
    arguments: &[&str],
    verdict_path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Output> {
    Command::new("git").args(arguments).current_dir(root).output().map_or_else(
        |error| {
            diagnostics.push(Diagnostic::at(
                verdict_path,
                format!("could not inspect detached-verdict Git binding: {error}"),
                "run verification in a repository with Git available and the reviewed objects fetched",
            ));
            None
        },
        Some,
    )
}
