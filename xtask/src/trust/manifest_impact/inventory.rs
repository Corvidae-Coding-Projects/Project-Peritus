use super::{ManifestContext, sha256_hex};
use crate::error::XtaskError;
use crate::trust::manifest_model::ProofImpactPackage;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const SHARED_INPUTS: &[&str] = &[
    ".cargo/config.toml",
    "Cargo.lock",
    "Cargo.toml",
    "architecture.toml",
    "rust-toolchain.toml",
    "toolchains.toml",
    "verification/actor-provenance.json",
    "verification/actors.toml",
    "verification/exclusions.toml",
    "verification/obligations.toml",
    "verification/trust.toml",
];

pub(super) fn expected_sources(
    context: &ManifestContext<'_>,
    compilation_sources: &[PathBuf],
) -> BTreeMap<PathBuf, Vec<ProofImpactPackage>> {
    let mut all_formal_packages = context
        .policy
        .packages
        .iter()
        .filter(|package| matches!(package.verification_class.as_str(), "V" | "H" | "T"))
        .map(|package| ProofImpactPackage {
            package: package.name.clone(),
            verification_class: package.verification_class.clone(),
        })
        .collect::<Vec<_>>();
    all_formal_packages.sort();
    let mut expected: BTreeMap<_, _> = compilation_sources
        .iter()
        .filter_map(|source| {
            let relative = source.strip_prefix(context.root).unwrap_or(source);
            context
                .policy
                .packages
                .iter()
                .filter(|package| relative.starts_with(&package.path))
                .max_by_key(|package| package.path.components().count())
                .filter(|package| matches!(package.verification_class.as_str(), "V" | "H" | "T"))
                .map(|package| {
                    (
                        relative.to_path_buf(),
                        vec![ProofImpactPackage {
                            package: package.name.clone(),
                            verification_class: package.verification_class.clone(),
                        }],
                    )
                })
        })
        .collect();
    for package in context
        .policy
        .packages
        .iter()
        .filter(|package| matches!(package.verification_class.as_str(), "V" | "H" | "T"))
    {
        if let Some(cargo_package) =
            context.cargo.packages.iter().find(|candidate| candidate.name == package.name)
        {
            let manifest = cargo_package
                .manifest_path
                .strip_prefix(context.root)
                .unwrap_or(&cargo_package.manifest_path)
                .to_path_buf();
            expected.insert(
                manifest,
                vec![ProofImpactPackage {
                    package: package.name.clone(),
                    verification_class: package.verification_class.clone(),
                }],
            );
        }
    }
    for path in SHARED_INPUTS {
        expected.insert(PathBuf::from(path), all_formal_packages.clone());
    }
    expected
}

/// Reads only discovered current inputs; stale declarations are inventory violations.
pub(super) fn current_sha256(
    context: &ManifestContext<'_>,
    relative: &Path,
    expected: &BTreeMap<PathBuf, Vec<ProofImpactPackage>>,
) -> Result<Option<String>, XtaskError> {
    if !expected.contains_key(relative) {
        return Ok(None);
    }
    let absolute = context.root.join(relative);
    let bytes = fs::read(&absolute).map_err(|error| XtaskError::io("read", &absolute, error))?;
    Ok(Some(sha256_hex(&bytes)))
}
