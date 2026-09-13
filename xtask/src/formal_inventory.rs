//! Source-derived audit inventory; this output is not proof discharge or review authorization.

use crate::error::XtaskError;
use crate::{metadata, trust};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const REGISTERS: [&str; 3] = ["obligations", "trust", "exclusions"];

#[cfg(test)]
mod tests;

pub(crate) fn render(root: &Path) -> Result<String, XtaskError> {
    let policy = metadata::architecture_policy(root)?;
    trust::check_local(root, &policy)?;
    let cargo = metadata::cargo_metadata(root)?;
    let mut registers = BTreeMap::new();
    let mut sources = BTreeSet::new();
    for register in REGISTERS {
        let path = format!("verification/{register}.toml");
        sources.insert(path.clone());
        let document: Value = metadata::read_toml(&root.join(&path))?;
        let entries = document["entries"]
            .as_array()
            .ok_or_else(|| XtaskError::metadata(format!("{path} has no entry inventory")))?;
        for entry in entries {
            collect_sources(entry, &mut sources);
            if let Some(evidence) = entry["evidence"].as_array() {
                for item in evidence {
                    collect_sources(item, &mut sources);
                }
            }
        }
        registers.insert(register, document);
    }
    let mut packages = BTreeMap::new();
    for package in &policy.packages {
        if !matches!(package.verification_class.as_str(), "V" | "H" | "T") {
            continue;
        }
        let manifest = format!("{}/Cargo.toml", package.path.display());
        sources.insert(manifest);
        let metadata =
            cargo.packages.iter().find(|entry| entry.name == package.name).ok_or_else(|| {
                XtaskError::metadata(format!("missing package `{}`", package.name))
            })?;
        let compiler_scope = read_scope(root, &package.name)?;
        let owning_obligations: Vec<_> = registers["obligations"]["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| entry["owning_crate"].as_str() == Some(&package.name))
            .filter_map(|entry| entry["id"].as_str())
            .collect();
        packages.insert(
            package.name.clone(),
            json!({
                "owner": package.owner,
                "verification_class": package.verification_class,
                "verus_opt_in": metadata.metadata.verus.as_ref().is_some_and(|value| value.verify),
                "owning_obligations": owning_obligations,
                "observed_compiler_scope": compiler_scope,
                "production_correspondence_review": "not discharged by this inventory",
            }),
        );
    }
    let fingerprints: BTreeMap<_, _> = sources
        .into_iter()
        .map(|relative| fingerprint(root, &relative).map(|digest| (relative, digest)))
        .collect::<Result<_, _>>()?;
    let report = json!({
        "schema": "peritus.formal-audit-inventory",
        "schema_version": 1,
        "status": "audit-only",
        "limits": [
            "Source fingerprints identify bytes read now, not independently reviewed source identities.",
            "Fingerprints cover register files, referenced source and evidence files, and formal package manifests; this is not a complete compilation-input snapshot.",
            "Compiler scopes are observations of the last local scope runs; freshness is not asserted here.",
            "A selected spec definition or solver query does not establish production correspondence.",
            "The registers enumerate declared boundaries; implicit upstream verifier, vstd, and solver trust still requires audit."
        ],
        "packages": packages,
        "registers": registers,
        "current_source_sha256": fingerprints,
    });
    serde_json::to_string_pretty(&report)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|error| XtaskError::metadata(format!("cannot render formal inventory: {error}")))
}

fn collect_sources(value: &Value, sources: &mut BTreeSet<String>) {
    if let Some(source) = value["source_file"].as_str() {
        sources.insert(source.to_owned());
    }
}

fn fingerprint(root: &Path, relative: &str) -> Result<String, XtaskError> {
    let path = root.join(relative);
    let bytes =
        fs::read(&path).map_err(|error| XtaskError::io("read audit source", &path, error))?;
    Ok(Sha256::digest(bytes).iter().fold(String::with_capacity(64), |mut output, byte| {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
        output
    }))
}

fn read_scope(root: &Path, package: &str) -> Result<Option<Value>, XtaskError> {
    let mut found = None;
    for shard in crate::ci_shard::SHARD_NAMES {
        let path = root
            .join("target/formal-scope")
            .join(shard)
            .join(format!("{package}-selected-functions.json"));
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(XtaskError::io("read observed compiler scope", &path, error)),
        };
        let value = crate::ci_shard::proof_observation(&bytes, package)?;
        if found.is_some() {
            return Err(XtaskError::metadata(format!("duplicate scope for `{package}`")));
        }
        found = Some(value);
    }
    Ok(found)
}
