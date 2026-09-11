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

pub(super) fn validate(
    context: &ManifestContext<'_>,
    current: &ProofImpactDocument,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<bool, XtaskError> {
    let Some(change) = current.changes.last() else { return Ok(false) };
    let Some(reference) = &change.verdict else { return Ok(false) };
    let Some(verdict) = manifest_file::read_toml::<ProofImpactVerdict>(
        context.root,
        Path::new(&reference.path),
        diagnostics,
    ) else {
        return Ok(false);
    };
    if !review_base::full_commit(&verdict.authorization_base_commit) {
        return Ok(false);
    }
    let Some(base) = review_base::load_base_manifest(
        context.root,
        &verdict.authorization_base_commit,
        diagnostics,
    )?
    else {
        return Ok(false);
    };
    if current.sources != base.sources {
        return Ok(false);
    }
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
    review_base::validate(context.root, current, diagnostics)?;
    Ok(true)
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
    use super::authorization_path;

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
}
