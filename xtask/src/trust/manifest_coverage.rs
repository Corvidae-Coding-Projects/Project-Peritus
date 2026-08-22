use super::manifest_actor::ActorRegistry;
use super::manifest_actor_model::ActorRole;
use super::manifest_context::ManifestContext;
use super::manifest_evidence::{validate_boundary_evidence, validate_proof_evidence};
use super::manifest_model::{
    ExclusionsDocument, ObligationEntry, ObligationStatus, ObligationsDocument,
};
use super::manifest_support::{
    source_line_exists, validate_envelope, validate_id, validate_issue, validate_review_window,
    validate_symbol, validate_symbol_declared_at_line, validate_text, validate_unique_id,
};
use crate::error::Diagnostic;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(super) fn validate_exclusions<'document>(
    context: &ManifestContext<'_>,
    actors: &ActorRegistry<'_>,
    document: &'document ExclusionsDocument,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<&'document str, (&'document str, &'document str)> {
    let manifest = Path::new("verification/exclusions.toml");
    validate_envelope(
        manifest,
        &document.schema,
        document.schema_version,
        &document.baseline,
        "peritus.verification.exclusions",
        diagnostics,
    );
    let mut ids = BTreeSet::new();
    let mut indexed = BTreeMap::new();
    for entry in &document.entries {
        validate_id(manifest, &entry.id, "EXCL-", diagnostics);
        validate_unique_id(manifest, &entry.id, &mut ids, diagnostics);
        indexed.insert(entry.id.as_str(), (entry.owning_crate.as_str(), entry.symbol.as_str()));
        for (field, value) in [
            ("unsupported_feature", entry.unsupported_feature.as_str()),
            ("risk", entry.risk.as_str()),
            ("upstream_tracking", entry.upstream_tracking.as_str()),
            ("revisit_plan", entry.revisit_plan.as_str()),
        ] {
            validate_text(manifest, &entry.id, field, value, diagnostics);
        }
        if context.package_class(&entry.owning_crate) != Some(entry.verification_class.as_str()) {
            diagnostics.push(Diagnostic::at(
                manifest,
                format!(
                    "entry `{}` class `{}` disagrees with its registered package",
                    entry.id,
                    entry.verification_class.as_str()
                ),
                "use an existing H/T package and its exact architecture class",
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
                "record the exact line at which the excluded item begins",
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
        validate_symbol_declared_at_line(
            manifest,
            &entry.id,
            source.as_deref(),
            entry.source_line,
            &entry.symbol,
            diagnostics,
        );
        if entry.evidence.is_empty() {
            diagnostics.push(Diagnostic::at(
                manifest,
                format!("entry `{}` has no compensating evidence", entry.id),
                "add independently executable evidence without claiming proof discharge",
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
            &entry.revisit_by,
            context.today,
            diagnostics,
        );
    }
    indexed
}

pub(super) fn validate_obligations(
    context: &ManifestContext<'_>,
    actors: &ActorRegistry<'_>,
    document: &ObligationsDocument,
    exclusions: &BTreeMap<&str, (&str, &str)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let manifest = Path::new("verification/obligations.toml");
    validate_envelope(
        manifest,
        &document.schema,
        document.schema_version,
        &document.baseline,
        "peritus.verification.obligations",
        diagnostics,
    );
    let mut ids = BTreeSet::new();
    for entry in &document.entries {
        validate_obligation_id(manifest, &entry.id, diagnostics);
        validate_unique_id(manifest, &entry.id, &mut ids, diagnostics);
        validate_text(manifest, &entry.id, "statement", &entry.statement, diagnostics);
        actors.validate_reference(
            manifest,
            &entry.id,
            "owner",
            &entry.owner,
            ActorRole::Owner,
            diagnostics,
        );
        validate_issue(manifest, &entry.id, &entry.live_issue, diagnostics);
        if context.package_class(&entry.owning_crate).is_none() {
            diagnostics.push(Diagnostic::at(
                manifest,
                format!("entry `{}` names unregistered package `{}`", entry.id, entry.owning_crate),
                "assign the obligation to one package in architecture.toml",
            ));
        }
        let source = context.validate_source(
            manifest,
            &entry.id,
            &entry.owning_crate,
            &entry.source_file,
            diagnostics,
        );
        validate_symbol(
            manifest,
            &entry.id,
            &entry.owning_crate,
            source.as_deref(),
            &entry.symbol,
            diagnostics,
        );
        validate_proof_evidence(
            context,
            manifest,
            &entry.id,
            &entry.owning_crate,
            &entry.evidence,
            diagnostics,
        );
        validate_status(context, actors, manifest, entry, exclusions, diagnostics);
        validate_dependencies(manifest, entry, diagnostics);
        let _kind = entry.kind.as_str();
    }
    validate_dependency_targets(manifest, &document.entries, diagnostics);
    validate_acyclic(manifest, &document.entries, diagnostics);
}

fn validate_obligation_id(manifest: &Path, id: &str, diagnostics: &mut Vec<Diagnostic>) {
    let invariant = id.strip_prefix("INV-").is_some_and(|digits| {
        digits.len() == 3 && digits.bytes().all(|byte| byte.is_ascii_digit()) && digits != "000"
    });
    let obligation = id.strip_prefix("OBL-").is_some_and(|digits| {
        digits.len() == 4 && digits.bytes().all(|byte| byte.is_ascii_digit()) && digits != "0000"
    });
    if !invariant && !obligation {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("obligation ID `{id}` is not `INV-NNN` or `OBL-NNNN`"),
            "use the canonical architecture invariant ID or assign a stable obligation ID",
        ));
    }
}

fn validate_status(
    context: &ManifestContext<'_>,
    actors: &ActorRegistry<'_>,
    manifest: &Path,
    entry: &ObligationEntry,
    exclusions: &BTreeMap<&str, (&str, &str)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match entry.status {
        ObligationStatus::Open | ObligationStatus::InProgress => {
            if entry.reviewer.is_some()
                || entry.review_date.is_some()
                || entry.exclusion_id.is_some()
            {
                status_error(
                    manifest,
                    entry,
                    "open/in-progress records cannot claim review or exclusion",
                    diagnostics,
                );
            }
        }
        ObligationStatus::Discharged => {
            let formal = entry.evidence.iter().any(|evidence| evidence.kind.is_formal());
            let complete = entry.reviewer.as_deref().zip(entry.review_date.as_deref());
            if !formal || complete.is_none() || entry.exclusion_id.is_some() {
                status_error(
                    manifest,
                    entry,
                    "discharged records require formal evidence and review, with no exclusion",
                    diagnostics,
                );
            }
            if let Some((reviewer, date)) = complete {
                actors.validate_pair(manifest, &entry.id, &entry.owner, reviewer, diagnostics);
                validate_review_window(
                    manifest,
                    &entry.id,
                    date,
                    "9999-12-31",
                    context.today,
                    diagnostics,
                );
            }
        }
        ObligationStatus::Excluded => {
            let matching = entry.exclusion_id.as_deref().and_then(|id| exclusions.get(id));
            if entry.reviewer.is_some()
                || entry.review_date.is_some()
                || matching != Some(&(entry.owning_crate.as_str(), entry.symbol.as_str()))
            {
                status_error(
                    manifest,
                    entry,
                    "excluded records require one matching live exclusion and no discharge review",
                    diagnostics,
                );
            }
        }
    }
}

fn status_error(
    manifest: &Path,
    entry: &ObligationEntry,
    message: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    diagnostics.push(Diagnostic::at(
        manifest,
        format!("entry `{}` status is inconsistent: {message}", entry.id),
        "use the exact conditional fields and evidence required by the declared status",
    ));
}

fn validate_dependencies(
    manifest: &Path,
    entry: &ObligationEntry,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let unique: BTreeSet<_> = entry.dependencies.iter().collect();
    if unique.len() != entry.dependencies.len() || unique.contains(&entry.id) {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{}` has duplicate or self dependencies", entry.id),
            "list each distinct prerequisite obligation once",
        ));
    }
}

fn validate_dependency_targets(
    manifest: &Path,
    entries: &[ObligationEntry],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let ids: BTreeSet<_> = entries.iter().map(|entry| entry.id.as_str()).collect();
    for entry in entries {
        for dependency in &entry.dependencies {
            if !ids.contains(dependency.as_str()) {
                diagnostics.push(Diagnostic::at(
                    manifest,
                    format!("entry `{}` depends on missing obligation `{dependency}`", entry.id),
                    "add the prerequisite record or remove the stale dependency",
                ));
            }
        }
    }
}

fn validate_acyclic(
    manifest: &Path,
    entries: &[ObligationEntry],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let graph: BTreeMap<_, _> =
        entries.iter().map(|entry| (entry.id.clone(), entry.dependencies.clone())).collect();
    if let Some(cycle) = find_cycle(&graph) {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("proof-obligation dependency graph contains a cycle: {}", cycle.join(" -> ")),
            "remove cyclic proof prerequisites so discharge order is well-founded",
        ));
    }
}

fn find_cycle(graph: &BTreeMap<String, Vec<String>>) -> Option<Vec<String>> {
    fn visit(
        node: &str,
        graph: &BTreeMap<String, Vec<String>>,
        complete: &mut BTreeSet<String>,
        stack: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if let Some(start) = stack.iter().position(|active| active == node) {
            let mut cycle = stack[start..].to_vec();
            cycle.push(node.to_owned());
            return Some(cycle);
        }
        if complete.contains(node) {
            return None;
        }
        stack.push(node.to_owned());
        for target in graph.get(node).into_iter().flatten().filter(|item| graph.contains_key(*item))
        {
            if let Some(cycle) = visit(target, graph, complete, stack) {
                return Some(cycle);
            }
        }
        stack.pop();
        complete.insert(node.to_owned());
        None
    }

    let mut complete = BTreeSet::new();
    for node in graph.keys() {
        if let Some(cycle) = visit(node, graph, &mut complete, &mut Vec::new()) {
            return Some(cycle);
        }
    }
    None
}
