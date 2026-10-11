//! Read-only current input inventory using the same discovery and ownership rules as the gate.
//! This deliberately neither consumes nor updates the possibly stale approval manifest.

use super::{ManifestContext, inventory, sha256_hex};
use crate::error::{ErrorCode, XtaskError};
use crate::{metadata, source, trust};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(crate) fn render(root: &Path) -> Result<String, XtaskError> {
    let policy = metadata::architecture_policy(root)?;
    let cargo = metadata::cargo_metadata(root)?;
    let (target_roots, mut diagnostics) = trust::workspace_target_policy(root, &cargo);
    let discovery = source::discover_compilation_sources(root, &policy, &target_roots)?;
    diagnostics.extend(discovery.diagnostics);
    if !diagnostics.is_empty() {
        return Err(XtaskError::violations(
            ErrorCode::Trust,
            "proof-impact-inventory",
            diagnostics,
        ));
    }
    let context = ManifestContext::new(root, &policy, &cargo);
    let sources = inventory::expected_sources(&context, &discovery.files)
        .into_iter()
        .map(|(path, packages)| {
            let absolute = root.join(&path);
            let bytes = fs::read(&absolute)
                .map_err(|error| XtaskError::io("read proof-impact input", &absolute, error))?;
            let affected: Vec<_> = packages
                .iter()
                .map(|package| {
                    json!({
                        "package": package.package,
                        "verification_class": package.verification_class,
                    })
                })
                .collect();
            Ok((path, json!({"sha256": sha256_hex(&bytes), "affected_packages": affected})))
        })
        .collect::<Result<BTreeMap<_, Value>, XtaskError>>()?;
    serde_json::to_string_pretty(&json!({
        "schema": "peritus.proof-impact-inventory",
        "schema_version": 1,
        "status": "audit-only",
        "hash_algorithm": "sha256-raw-bytes-v1",
        "limits": [
            "This is the current proof-impact input set, not approved change history or a complete compiler snapshot.",
            "Package sets follow the gate's direct ownership and shared-input policy; they are not transitive dependency impact.",
            "No proof, test, review, protected-base authorization, or obligation discharge is asserted."
        ],
        "sources": sources,
    }))
    .map(|mut output| {
        output.push('\n');
        output
    })
    .map_err(|error| XtaskError::metadata(format!("cannot render proof-impact inventory: {error}")))
}
