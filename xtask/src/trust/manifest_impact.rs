use super::manifest_actor::ActorRegistry;
use super::manifest_context::ManifestContext;
use super::manifest_date::CalendarDate;
use super::manifest_model::{
    ProofImpactChange, ProofImpactDocument, ProofImpactPackage, ProofImpactSnapshot,
    ProofImpactSource, ProofImpactStatus, ProofSourceChange,
};
use super::manifest_support::validate_text;
use crate::error::{Diagnostic, XtaskError};
use inventory::expected_sources;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const MANIFEST: &str = "verification/proof-impact.toml";
const SCHEMA: &str = "peritus.verification.proof-impact";
const HASH_ALGORITHM: &str = "sha256-raw-bytes-v1";

pub(super) fn validate(
    context: &ManifestContext<'_>,
    actors: &ActorRegistry<'_>,
    document: &ProofImpactDocument,
    compilation_sources: &[PathBuf],
    enforce_review_base: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    validate_envelope(document, diagnostics);
    let expected = expected_sources(context, compilation_sources);
    let changes = validate_changes(context, actors, document, diagnostics);
    verdict::validate_directory(context.root, document, diagnostics);
    validate_source_inventory(context, document, &expected, &changes, diagnostics)?;
    if enforce_review_base {
        review_base::validate(context.root, document, diagnostics)?;
    }
    Ok(())
}

fn validate_envelope(document: &ProofImpactDocument, diagnostics: &mut Vec<Diagnostic>) {
    if document.schema != SCHEMA
        || document.schema_version != 1
        || document.baseline != "A1"
        || document.hash_algorithm != HASH_ALGORITHM
    {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            "proof-impact manifest envelope differs from the reviewed A1 raw-byte SHA-256 schema",
            format!(
                "use schema `{SCHEMA}`, schema_version 1, baseline `A1`, and `{HASH_ALGORITHM}`"
            ),
        ));
    }
}

fn validate_changes<'a>(
    context: &ManifestContext<'_>,
    actors: &ActorRegistry<'_>,
    document: &'a ProofImpactDocument,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<&'a str, &'a ProofImpactChange> {
    let mut changes = BTreeMap::new();
    let mut history = BTreeMap::<&str, (Option<&ProofImpactSnapshot>, &str)>::new();
    for change in &document.changes {
        validate_change(context, actors, change, diagnostics);
        if changes.insert(change.id.as_str(), change).is_some() {
            diagnostics.push(Diagnostic::at(
                MANIFEST,
                format!("proof-impact change ID `{}` is declared more than once", change.id),
                "retain one immutable review record per stable PCR-NNNN ID",
            ));
        }
        for source_change in &change.source_changes {
            validate_chain_link(change, source_change, &mut history, diagnostics);
        }
    }
    changes
}

fn validate_change(
    context: &ManifestContext<'_>,
    actors: &ActorRegistry<'_>,
    change: &ProofImpactChange,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !valid_change_id(&change.id) {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("proof-impact change ID `{}` is not nonzero PCR-NNNN form", change.id),
            "assign one stable four-digit proof-change review ID",
        ));
    }
    for (field, value) in [("rationale", &change.rationale), ("impact", &change.impact)] {
        validate_text(Path::new(MANIFEST), &change.id, field, value, diagnostics);
    }
    actors.validate_pair(
        Path::new(MANIFEST),
        &change.id,
        &change.owner,
        &change.reviewer,
        diagnostics,
    );
    validate_review_date(context, change, diagnostics);
    if change.source_changes.is_empty() || change.change_kinds.is_empty() {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("change `{}` has no exact source changes or impact kinds", change.id),
            "list every changed source fingerprint and each applicable formal change kind",
        ));
    }
    validate_unique_change_values(change, diagnostics);
    for source in &change.source_changes {
        validate_transition(&change.id, source, diagnostics);
    }
    evidence::validate(change, diagnostics);
    verdict::validate_change(context.root, actors, change, diagnostics);
}

fn validate_review_date(
    context: &ManifestContext<'_>,
    change: &ProofImpactChange,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let reviewed = CalendarDate::parse(&change.review_date);
    if reviewed.is_none_or(|date| date > context.today) {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("change `{}` has a malformed or future review date", change.id),
            "record the completed independent review date in exact YYYY-MM-DD form",
        ));
    }
}

fn validate_unique_change_values(change: &ProofImpactChange, diagnostics: &mut Vec<Diagnostic>) {
    let kinds: BTreeSet<_> = change.change_kinds.iter().collect();
    let required = BTreeSet::from([
        &super::manifest_model::ProofImpactKind::Executable,
        &super::manifest_model::ProofImpactKind::Specification,
        &super::manifest_model::ProofImpactKind::Precondition,
        &super::manifest_model::ProofImpactKind::Postcondition,
        &super::manifest_model::ProofImpactKind::Proof,
    ]);
    let sources: BTreeSet<_> =
        change.source_changes.iter().map(|source| source.source_file.as_str()).collect();
    if kinds.len() != change.change_kinds.len() || sources.len() != change.source_changes.len() {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("change `{}` repeats an impact kind or source", change.id),
            "record each exact impact kind and source transition once",
        ));
    }
    if kinds != required {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("change `{}` does not declare the complete conservative impact set", change.id),
            "A1 has no trusted Verus parser, so every raw-byte source change requires executable, specification, precondition, postcondition, and proof review",
        ));
    }
}

fn validate_chain_link<'a>(
    change: &'a ProofImpactChange,
    source_change: &'a ProofSourceChange,
    history: &mut BTreeMap<&'a str, (Option<&'a ProofImpactSnapshot>, &'a str)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if source_change.previous == source_change.current {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("change `{}` contains an empty or unchanged input transition", change.id),
            "record an addition, removal, byte change, or affected-package scope change",
        ));
    }
    match history.get(source_change.source_file.as_str()) {
        None if source_change.previous.is_some() => {
            chain_error(change, source_change, diagnostics);
        }
        Some((previous, _)) if source_change.previous.as_ref() != *previous => {
            chain_error(change, source_change, diagnostics);
        }
        _ => {}
    }
    history.insert(
        source_change.source_file.as_str(),
        (source_change.current.as_ref(), change.id.as_str()),
    );
}

fn validate_transition(
    change_id: &str,
    source: &ProofSourceChange,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for snapshot in [source.previous.as_ref(), source.current.as_ref()].into_iter().flatten() {
        validate_snapshot(change_id, snapshot, diagnostics);
    }
}

fn validate_snapshot(
    entry: &str,
    snapshot: &ProofImpactSnapshot,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !valid_sha256(&snapshot.sha256) {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("proof-impact entry `{entry}` contains a malformed SHA-256 fingerprint"),
            "use lowercase 64-hex SHA-256 over the exact raw input bytes",
        ));
    }
    validate_packages(entry, &snapshot.affected_packages, diagnostics);
}

fn validate_packages(
    entry: &str,
    packages: &[ProofImpactPackage],
    diagnostics: &mut Vec<Diagnostic>,
) {
    if packages.is_empty() || packages.windows(2).any(|pair| pair[0] >= pair[1]) {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("proof-impact entry `{entry}` has a noncanonical affected-package set"),
            "list each affected package exactly once in ascending package/class order",
        ));
    }
    for package in packages {
        if !valid_package_name(&package.package)
            || !matches!(package.verification_class.as_str(), "V" | "H" | "T")
        {
            diagnostics.push(Diagnostic::at(
                MANIFEST,
                format!(
                    "proof-impact entry `{entry}` declares malformed affected package `{}` class `{}`",
                    package.package, package.verification_class
                ),
                "use an exact Cargo package name and formal V/H/T class",
            ));
        }
    }
}

fn chain_error(
    change: &ProofImpactChange,
    source_change: &ProofSourceChange,
    diagnostics: &mut Vec<Diagnostic>,
) {
    diagnostics.push(Diagnostic::at(
        MANIFEST,
        format!(
            "change `{}` does not continue the fingerprint chain for `{}`",
            change.id, source_change.source_file
        ),
        "use no previous hash only for the first baseline, then link the exact prior current hash",
    ));
}

fn validate_source_inventory(
    context: &ManifestContext<'_>,
    document: &ProofImpactDocument,
    expected: &BTreeMap<PathBuf, Vec<ProofImpactPackage>>,
    changes: &BTreeMap<&str, &ProofImpactChange>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let mut declared = BTreeSet::new();
    for source in &document.sources {
        validate_source(context, source, expected, changes, diagnostics)?;
        if !declared.insert(PathBuf::from(&source.source_file)) {
            diagnostics.push(Diagnostic::at(
                MANIFEST,
                format!("source `{}` has more than one current fingerprint", source.source_file),
                "retain exactly one current baseline record per formal compilation source",
            ));
        }
    }
    for source in expected.keys().filter(|source| !declared.contains(*source)) {
        diagnostics.push(Diagnostic::at(
            source,
            "formal semantics input has no proof-impact fingerprint",
            "add its exact SHA-256 and independently reviewed initial/change record",
        ));
    }
    for source in declared.iter().filter(|source| !expected.contains_key(*source)) {
        diagnostics.push(Diagnostic::at(
            source,
            "proof-impact fingerprint is stale or outside a V/H/T semantics input",
            "remove the stale record or restore the registered formal source/package manifest",
        ));
    }
    Ok(())
}

fn validate_source(
    context: &ManifestContext<'_>,
    source: &ProofImpactSource,
    expected: &BTreeMap<PathBuf, Vec<ProofImpactPackage>>,
    changes: &BTreeMap<&str, &ProofImpactChange>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let relative = Path::new(&source.source_file);
    validate_packages(&source.source_file, &source.affected_packages, diagnostics);
    if expected.get(relative) != Some(&source.affected_packages) {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!(
                "input `{}` affected-package set does not match architecture policy",
                source.source_file
            ),
            "record every exact registered V/H/T package and class affected by this input",
        ));
    }
    if !valid_sha256(&source.sha256) {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("source `{}` has a malformed SHA-256", source.source_file),
            "record lowercase 64-hex SHA-256 over the exact raw source bytes",
        ));
    }
    let absolute = context.root.join(relative);
    let bytes = fs::read(&absolute).map_err(|error| XtaskError::io("read", &absolute, error))?;
    let actual = sha256_hex(&bytes);
    if actual != source.sha256 {
        diagnostics.push(Diagnostic::at(
            relative,
            "formal source bytes differ from the reviewed proof-impact fingerprint",
            "add an independently reviewed chained change record and update the current SHA-256",
        ));
    }
    let linked = changes.get(source.change_id.as_str());
    let transition = linked.and_then(|change| {
        change.source_changes.iter().find(|item| {
            item.source_file == source.source_file
                && item.current.as_ref().is_some_and(|current| {
                    current.sha256 == source.sha256
                        && current.affected_packages == source.affected_packages
                })
        })
    });
    if linked.is_none_or(|change| change.status != ProofImpactStatus::Approved)
        || transition.is_none()
    {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!(
                "source `{}` is not linked to an approved exact-hash change",
                source.source_file
            ),
            "reference the latest approved PCR-NNNN record containing this exact source transition",
        ));
    }
    Ok(())
}

fn valid_change_id(id: &str) -> bool {
    id.strip_prefix("PCR-").is_some_and(|digits| {
        digits.len() == 4 && digits != "0000" && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn valid_package_name(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().fold(String::with_capacity(64), |mut output, byte| {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
        output
    })
}

#[path = "manifest_impact/evidence.rs"]
mod evidence;
#[path = "manifest_impact/inventory.rs"]
mod inventory;
#[path = "manifest_impact/review_base.rs"]
mod review_base;
#[path = "manifest_impact/verdict.rs"]
mod verdict;
