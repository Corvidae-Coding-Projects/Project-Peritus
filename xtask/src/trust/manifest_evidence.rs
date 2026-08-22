use super::manifest_context::ManifestContext;
use super::manifest_model::{BoundaryEvidence, ProofEvidence};
use super::manifest_support::validate_symbol;
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
    validate_declaration(manifest, id, source.as_deref(), symbol, command, diagnostics);
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
    source: Option<&Path>,
    symbol: &str,
    command: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(source) = source else { return };
    let Ok(contents) = fs::read_to_string(source) else { return };
    let name = symbol.rsplit("::").next().unwrap_or(symbol);
    let lines: Vec<_> = contents.lines().collect();
    let declaration = lines.iter().position(|line| declaration_name(line) == Some(name));
    let Some(index) = declaration else { return };
    let attributes = lines[..index]
        .iter()
        .rev()
        .take_while(|line| line.trim().starts_with("#[") || line.trim().is_empty())
        .copied()
        .collect::<Vec<_>>();
    let executable_test = attributes.iter().any(|line| line.trim() == "#[test]")
        && !attributes.iter().any(|line| line.trim().starts_with("#[cfg"));
    let formal = lines[index].contains("proof fn") || lines[index].contains("spec fn");
    let valid = if command.starts_with("cargo test ") { executable_test } else { formal };
    if !valid {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{id}` evidence symbol `{symbol}` is not exercised by its command"),
            "use an unconditional #[test] for cargo test or an exact proof/spec item for Cargo-Verus",
        ));
    }
}

fn declaration_name(line: &str) -> Option<&str> {
    let tokens: Vec<_> = line
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| !token.is_empty())
        .collect();
    tokens.windows(2).find_map(|pair| {
        ["fn", "struct", "enum", "union", "trait", "type", "const", "static"]
            .contains(&pair[0])
            .then_some(pair[1])
    })
}
