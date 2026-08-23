use super::MANIFEST;
use crate::error::{Diagnostic, XtaskError};
use crate::trust::manifest_model::{
    ProofImpactChange, ProofImpactDocument, ProofImpactKind, ProofImpactSnapshot,
    ProofImpactSource, ProofImpactStatus,
};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const BASE_ENV: &str = "PERITUS_PROOF_IMPACT_BASE";

pub(super) fn validate(
    root: &Path,
    current: &ProofImpactDocument,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let Some(base_commit) = resolve_base(root, diagnostics)? else { return Ok(()) };
    let Some(base) = load_base_manifest(root, &base_commit, diagnostics)? else {
        validate_genesis(root, current, diagnostics)?;
        return Ok(());
    };
    validate_immutable_history(&base, current, diagnostics);
    super::verdict::validate_new_authorization_bases(
        root,
        &base_commit,
        &base,
        current,
        diagnostics,
    );
    validate_transitions(root, &base_commit, &base, current, diagnostics)?;
    Ok(())
}

fn resolve_base(
    root: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<String>, XtaskError> {
    let github = env::var("GITHUB_ACTIONS").is_ok_and(|value| value == "true");
    let requested = env::var(BASE_ENV).ok().filter(|value| !value.is_empty());
    if github && requested.as_deref().is_none_or(|value| !full_commit(value)) {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("GitHub Actions requires `{BASE_ENV}` to be a nonzero full commit SHA"),
            "pass the immutable pull-request base SHA, push before-SHA, or required dispatch input",
        ));
        return Ok(None);
    }
    let reference = requested.as_deref().unwrap_or("HEAD");
    if requested.is_some() && !full_commit(reference) {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("`{BASE_ENV}` is not a nonzero full commit SHA"),
            "use the exact 40-hex protected-base commit",
        ));
        return Ok(None);
    }
    let base = git(root, &["rev-parse", "--verify", &format!("{reference}^{{commit}}")])?;
    if !base.status.success() {
        diagnostics.push(base_error("does not resolve to a Git commit"));
        return Ok(None);
    }
    let base = String::from_utf8_lossy(&base.stdout).trim().to_owned();
    let head = git(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let head = String::from_utf8_lossy(&head.stdout).trim().to_owned();
    let ancestor = git(root, &["merge-base", "--is-ancestor", &base, "HEAD"])?;
    if !ancestor.status.success() {
        diagnostics.push(base_error("is not an ancestor of HEAD"));
    }
    if github && base == head {
        diagnostics.push(base_error("must differ from HEAD in GitHub Actions"));
    }
    Ok(Some(base))
}

fn load_base_manifest(
    root: &Path,
    base: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<ProofImpactDocument>, XtaskError> {
    let object = format!("{base}:{MANIFEST}");
    let output = git(root, &["show", &object])?;
    if !output.status.success() {
        return Ok(None);
    }
    let contents = String::from_utf8_lossy(&output.stdout);
    toml::from_str(&contents).map_or_else(
        |_| {
            diagnostics
                .push(base_error("contains an invalid protected-base proof-impact manifest"));
            Ok(None)
        },
        |document| Ok(Some(document)),
    )
}

fn validate_genesis(
    root: &Path,
    current: &ProofImpactDocument,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let parents = git(root, &["rev-list", "--parents", "-n", "1", "HEAD"])?;
    let parent_has_history =
        String::from_utf8_lossy(&parents.stdout).split_ascii_whitespace().skip(1).any(|parent| {
            git(root, &["log", "-1", "--format=%H", parent, "--", MANIFEST])
                .is_ok_and(|output| !output.stdout.is_empty())
        });
    if current.baseline != "A1"
        || current.changes.is_empty()
        || current.sources.is_empty()
        || current.changes.iter().any(|change| change.status != ProofImpactStatus::Approved)
        || parent_has_history
    {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            "the one-time proof-impact genesis is incomplete",
            "establish the reviewed A1 source inventory before protected-base enforcement begins",
        ));
    }
    Ok(())
}

fn validate_immutable_history(
    base: &ProofImpactDocument,
    current: &ProofImpactDocument,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if current.changes.get(..base.changes.len()) != Some(base.changes.as_slice()) {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            "protected proof-impact records are not an exact immutable prefix",
            "restore their content and order; append every new review record after protected history",
        ));
    }
}

fn validate_transitions(
    root: &Path,
    base_commit: &str,
    base: &ProofImpactDocument,
    current: &ProofImpactDocument,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let old_sources: BTreeMap<_, _> =
        base.sources.iter().map(|source| (source.source_file.as_str(), source)).collect();
    let new_sources: BTreeMap<_, _> =
        current.sources.iter().map(|source| (source.source_file.as_str(), source)).collect();
    let paths: BTreeSet<_> = old_sources.keys().chain(new_sources.keys()).copied().collect();
    for path in paths {
        let before = old_sources.get(path).copied();
        let after = new_sources.get(path).copied();
        if same_identity(before, after) {
            if old_sources.get(path) != new_sources.get(path) {
                diagnostics.push(Diagnostic::at(
                    MANIFEST,
                    format!("unchanged input `{path}` rewrites its protected inventory identity"),
                    "retain its affected packages, hash, and latest reviewed change ID exactly",
                ));
            }
            continue;
        }
        let candidate = transition_for(path, before, after, current);
        let authorized = candidate.filter(|change| pre_authorized(change, base));
        let Some(change) = authorized else {
            diagnostics.push(Diagnostic::at(
                path,
                "formal source transition was not pre-authorized on the protected Git base",
                "land the exact-hash independent review record first, then apply the source change",
            ));
            continue;
        };
        validate_change_kinds(root, base_commit, path, change, diagnostics)?;
    }
    Ok(())
}

fn pre_authorized(change: &ProofImpactChange, base: &ProofImpactDocument) -> bool {
    change.status == ProofImpactStatus::Approved && base.changes.iter().any(|old| old == change)
}

fn transition_for<'a>(
    path: &str,
    before: Option<&ProofImpactSource>,
    after: Option<&ProofImpactSource>,
    document: &'a ProofImpactDocument,
) -> Option<&'a ProofImpactChange> {
    document.changes.iter().find(|change| {
        change.source_changes.iter().any(|source| {
            source.source_file == path
                && snapshot_matches(source.previous.as_ref(), before)
                && snapshot_matches(source.current.as_ref(), after)
        })
    })
}

fn same_identity(before: Option<&ProofImpactSource>, after: Option<&ProofImpactSource>) -> bool {
    match (before, after) {
        (Some(before), Some(after)) => {
            before.sha256 == after.sha256 && before.affected_packages == after.affected_packages
        }
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
    }
}

fn snapshot_matches(
    snapshot: Option<&ProofImpactSnapshot>,
    source: Option<&ProofImpactSource>,
) -> bool {
    match (snapshot, source) {
        (Some(snapshot), Some(source)) => {
            snapshot.sha256 == source.sha256
                && snapshot.affected_packages == source.affected_packages
        }
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
    }
}

fn validate_change_kinds(
    root: &Path,
    base: &str,
    path: &str,
    change: &ProofImpactChange,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let old = git(root, &["show", &format!("{base}:{path}")])?;
    let old = if old.status.success() { old.stdout } else { Vec::new() };
    let new = fs::read(root.join(path)).unwrap_or_default();
    let required = classify(&old, &new);
    let declared: BTreeSet<_> = change.change_kinds.iter().copied().collect();
    for kind in required.difference(&declared) {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("change `{}` omits derived `{kind:?}` impact for `{path}`", change.id),
            "record every independently derived executable, specification, contract, and proof impact",
        ));
    }
    Ok(())
}

fn classify(old: &[u8], new: &[u8]) -> BTreeSet<ProofImpactKind> {
    if old == new {
        return BTreeSet::new();
    }
    // A1 deliberately has no trusted Verus parser. Any byte change can move tokens across
    // executable/spec/proof contexts, so the safe classifier requires review of every category.
    BTreeSet::from([
        ProofImpactKind::Executable,
        ProofImpactKind::Specification,
        ProofImpactKind::Precondition,
        ProofImpactKind::Postcondition,
        ProofImpactKind::Proof,
    ])
}

fn git(root: &Path, arguments: &[&str]) -> Result<Output, XtaskError> {
    Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| XtaskError::io("execute Git from", root, error))
}

fn full_commit(value: &str) -> bool {
    value.len() == 40
        && value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        && value.bytes().any(|byte| byte != b'0')
}

fn base_error(detail: &str) -> Diagnostic {
    Diagnostic::at(
        MANIFEST,
        format!("protected proof-impact base {detail}"),
        format!("set `{BASE_ENV}` to an available nonzero 40-hex ancestor commit"),
    )
}

#[cfg(test)]
#[path = "review_base/tests.rs"]
mod tests;
