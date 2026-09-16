use super::manifest_context::ManifestContext;
use super::manifest_model::{BoundaryEvidence, ProofEvidence};
use super::manifest_support::validate_symbol;
use super::manifest_symbol::owned_function_declarations;
use crate::api_contract::{CargoTest, Configuration, Mode};
use crate::error::Diagnostic;
use crate::reproducibility;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub(super) fn validate_boundary_evidence(
    context: &ManifestContext<'_>,
    manifest: &Path,
    id: &str,
    owning_crate: &str,
    evidence: &[BoundaryEvidence],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen = BTreeSet::new();
    for item in evidence {
        if !seen.insert((&item.source_file, &item.symbol, &item.command)) {
            diagnostics.push(Diagnostic::at(
                manifest,
                format!("entry `{id}` repeats an evidence locator"),
                "retain each independently executable evidence record once",
            ));
        }
        validate_item(
            context,
            manifest,
            id,
            owning_crate,
            &item.source_file,
            &item.symbol,
            &item.command,
            diagnostics,
        );
    }
}

pub(super) fn validate_proof_evidence(
    context: &ManifestContext<'_>,
    manifest: &Path,
    id: &str,
    owning_crate: &str,
    evidence: &[ProofEvidence],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for item in evidence {
        validate_item(
            context,
            manifest,
            id,
            owning_crate,
            &item.source_file,
            &item.symbol,
            &item.command,
            diagnostics,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_item(
    context: &ManifestContext<'_>,
    manifest: &Path,
    id: &str,
    owning_crate: &str,
    source_file: &str,
    symbol: &str,
    command: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let source = context.validate_source(manifest, id, owning_crate, source_file, diagnostics);
    validate_symbol(manifest, id, owning_crate, source.as_deref(), symbol, diagnostics);
    validate_declaration(
        manifest,
        id,
        owning_crate,
        source.as_deref(),
        symbol,
        command,
        diagnostics,
    );
    if !reproducibility::is_exact_evidence_command(
        command,
        owning_crate,
        context.package_class(owning_crate).unwrap_or(""),
    ) {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{id}` evidence command is not an exact locked package gate"),
            "use the canonical test or the class-specific full cargo-verus package command",
        ));
    }
}

fn validate_declaration(
    manifest: &Path,
    id: &str,
    owning_crate: &str,
    source: Option<&Path>,
    symbol: &str,
    command: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(source) = source else { return };
    let Ok(contents) = fs::read_to_string(source) else { return };
    let name = symbol.rsplit("::").next().unwrap_or(symbol);
    let declarations = owned_function_declarations(owning_crate, source, &contents, name);
    let mut exact = declarations.iter().filter(|declaration| declaration.path == symbol);
    let Some(declaration) = exact.next() else { return };
    if exact.next().is_some() {
        return;
    }
    let declaration = declaration.declaration;
    let valid = if command.starts_with("cargo test ") {
        declaration.cargo_test == CargoTest::Runnable
            && declaration.configuration != Configuration::Other
            && !declaration.nested
    } else if command.starts_with("cargo verus ") {
        declaration.in_verus
            && declaration.configuration == Configuration::Unconditional
            && !declaration.nested
            && matches!(declaration.mode, Some(Mode::Exec | Mode::Proof | Mode::Spec))
    } else {
        false
    };
    if !valid {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{id}` evidence symbol `{symbol}` is not exercised by its command"),
            "use a non-ignored #[test] enabled unconditionally or only by cfg(test), or an unconditional function declared inside verus! whose mode is confirmed by the fresh compiler proof-scope gate",
        ));
    }
}
