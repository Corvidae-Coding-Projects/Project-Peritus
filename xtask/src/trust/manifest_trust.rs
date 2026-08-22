use super::construct::Construct;
use super::manifest::TrustedOccurrence;
use super::manifest_actor::ActorRegistry;
use super::manifest_context::ManifestContext;
use super::manifest_evidence::validate_boundary_evidence;
use super::manifest_model::{TrustDocument, TrustEntry};
use super::manifest_support::{
    source_line_exists, validate_envelope, validate_id, validate_issue, validate_review_window,
    validate_symbol, validate_symbol_governs_line, validate_text, validate_unique_id,
    version_is_pinned,
};
use crate::error::Diagnostic;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(super) fn validate(
    context: &ManifestContext<'_>,
    actors: &ActorRegistry<'_>,
    document: &TrustDocument,
    occurrences: &[TrustedOccurrence],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let manifest = Path::new("verification/trust.toml");
    validate_envelope(
        manifest,
        &document.schema,
        document.schema_version,
        &document.baseline,
        "peritus.verification.trust",
        diagnostics,
    );
    let mut ids = BTreeSet::new();
    let mut entry_keys = BTreeMap::<(&str, u64, &str, &str), usize>::new();

    for entry in &document.entries {
        validate_entry(context, actors, manifest, entry, &mut ids, diagnostics);
        *entry_keys
            .entry((&entry.source_file, entry.source_line, &entry.construct_kind, &entry.symbol))
            .or_default() += 1;
    }

    reconcile(manifest, occurrences, &entry_keys, diagnostics);
}

fn validate_entry(
    context: &ManifestContext<'_>,
    actors: &ActorRegistry<'_>,
    manifest: &Path,
    entry: &TrustEntry,
    ids: &mut BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    validate_id(manifest, &entry.id, "TRUST-", diagnostics);
    validate_unique_id(manifest, &entry.id, ids, diagnostics);
    for (field, value) in [
        ("upstream", entry.upstream.as_str()),
        ("assumed_contract", entry.assumed_contract.as_str()),
        ("threat_if_false", entry.threat_if_false.as_str()),
    ] {
        validate_text(manifest, &entry.id, field, value, diagnostics);
    }
    if entry.owning_crate != "peritus-tcb"
        || !Path::new(&entry.source_file).starts_with("crates/foundation/peritus-tcb/src")
    {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{}` is outside the sole peritus-tcb source boundary", entry.id),
            "place the exact construct under crates/foundation/peritus-tcb/src and declare that owning crate",
        ));
    }
    let source = context.validate_source(
        manifest,
        &entry.id,
        &entry.owning_crate,
        &entry.source_file,
        diagnostics,
    );
    if source.as_deref().is_some_and(|path| !source_line_exists(path, entry.source_line)) {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{}` source_line is outside its current source file", entry.id),
            "record the exact positive line of the trusted occurrence",
        ));
    }
    validate_symbol(
        manifest,
        &entry.id,
        &entry.owning_crate,
        source.as_deref(),
        &entry.symbol,
        diagnostics,
    );
    validate_symbol_governs_line(
        manifest,
        &entry.id,
        source.as_deref(),
        entry.source_line,
        &entry.symbol,
        &entry.construct_kind,
        diagnostics,
    );
    if !Construct::is_known_label(&entry.construct_kind) {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{}` names unknown construct `{}`", entry.id, entry.construct_kind),
            "use one exact construct label emitted by the trust scanner",
        ));
    }
    if !version_is_pinned(&entry.upstream_version) {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{}` has a floating or non-versioned upstream reference", entry.id),
            "record an immutable revision, release, ABI, or explicit platform-version range",
        ));
    }
    if entry.evidence.is_empty()
        || !entry.evidence.iter().any(|item| item.kind.is_refinement_or_conformance())
    {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{}` lacks refinement or conformance evidence", entry.id),
            "record at least one executable refinement-test or conformance-test locator",
        ));
    }
    validate_boundary_evidence(
        context,
        manifest,
        &entry.id,
        &entry.owning_crate,
        &entry.evidence,
        diagnostics,
    );
    validate_issue(manifest, &entry.id, &entry.live_issue, diagnostics);
    actors.validate_pair(manifest, &entry.id, &entry.owner, &entry.reviewer, diagnostics);
    validate_review_window(
        manifest,
        &entry.id,
        &entry.review_date,
        &entry.expiry_date,
        context.today,
        diagnostics,
    );
}

fn reconcile(
    manifest: &Path,
    occurrences: &[TrustedOccurrence],
    entries: &BTreeMap<(&str, u64, &str, &str), usize>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut actual = BTreeMap::<(&str, u64, &str, &str), usize>::new();
    for occurrence in occurrences {
        *actual
            .entry((
                occurrence.source.to_str().unwrap_or("<non-utf8>"),
                occurrence.line,
                occurrence.construct,
                occurrence.symbol.as_str(),
            ))
            .or_default() += 1;
    }
    for (key, count) in entries {
        if *count != 1 || actual.get(key) != Some(&1) {
            diagnostics.push(Diagnostic::at(
                manifest,
                format!(
                    "trusted entry at {}:{} `{}` for `{}` does not match exactly one source occurrence",
                    key.0, key.1, key.2, key.3
                ),
                "update or remove the stale/duplicate entry; broad and ambiguous allowlists are forbidden",
            ));
        }
    }
    for (key, count) in actual {
        if count != 1 || entries.get(&key) != Some(&1) {
            diagnostics.push(Diagnostic::at(
                key.0,
                format!(
                    "line {} trusted construct `{}` in `{}` has no unique manifest entry",
                    key.1, key.2, key.3
                ),
                "add one fully reviewed trust.toml record or remove the trusted construct",
            ));
        }
    }
}
