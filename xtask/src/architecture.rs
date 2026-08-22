use crate::error::{Diagnostic, ErrorCode, XtaskError};
use crate::metadata;
use crate::model::{ArchitecturePolicy, CargoMetadata, CargoPackage, PackagePolicy};
use crate::source;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

mod dependency;
mod policy;

use dependency::validate_dependency_edges;
use policy::validate_policy;

pub(crate) fn check(
    root: &Path,
    policy: &ArchitecturePolicy,
) -> Result<(usize, usize), XtaskError> {
    let mut diagnostics = validate_policy(policy);
    let cargo = metadata::cargo_metadata(root)?;
    diagnostics.extend(validate_packages(root, policy, &cargo)?);
    diagnostics.extend(validate_verus_opt_ins(root, policy, &cargo));
    let source_count = source::check(root, policy, &cargo)?;

    if diagnostics.is_empty() {
        Ok((cargo.workspace_members.len(), source_count))
    } else {
        Err(XtaskError::violations(ErrorCode::Architecture, "architecture-check", diagnostics))
    }
}

pub(crate) fn validate_verus_opt_ins(
    root: &Path,
    policy: &ArchitecturePolicy,
    cargo: &CargoMetadata,
) -> Vec<Diagnostic> {
    let workspace_ids: BTreeSet<_> = cargo.workspace_members.iter().map(String::as_str).collect();
    let policy_by_name: BTreeMap<_, _> =
        policy.packages.iter().map(|package| (package.name.as_str(), package)).collect();
    let mut diagnostics = Vec::new();

    for package in
        cargo.packages.iter().filter(|package| workspace_ids.contains(package.id.as_str()))
    {
        let Some(expected) = policy_by_name.get(package.name.as_str()) else {
            diagnostics.push(Diagnostic::at(
                relative(root, &package.manifest_path),
                format!(
                    "workspace package `{}` has no verification class, so Verus opt-in cannot be established",
                    package.name
                ),
                "register the package in architecture.toml before it enters the workspace",
            ));
            continue;
        };
        if matches!(expected.verification_class.as_str(), "V" | "H" | "T")
            && package.metadata.verus.as_ref().is_none_or(|metadata| !metadata.is_plain_verified())
        {
            diagnostics.push(Diagnostic::at(
                relative(root, &package.manifest_path),
                format!(
                    "verification class `{}` package `{}` is opted out of Cargo-Verus",
                    expected.verification_class, package.name
                ),
                "set exactly [package.metadata.verus] verify = true; bootstrap/no-vstd flags change the trusted verification mode",
            ));
        }
    }
    diagnostics
}

fn validate_packages(
    root: &Path,
    policy: &ArchitecturePolicy,
    cargo: &CargoMetadata,
) -> Result<Vec<Diagnostic>, XtaskError> {
    let mut diagnostics = Vec::new();
    let workspace_ids: BTreeSet<_> = cargo.workspace_members.iter().map(String::as_str).collect();
    let packages: Vec<_> = cargo
        .packages
        .iter()
        .filter(|package| workspace_ids.contains(package.id.as_str()))
        .collect();
    let cargo_by_name: BTreeMap<_, _> =
        packages.iter().map(|package| (package.name.as_str(), *package)).collect();
    let policy_by_name: BTreeMap<_, _> =
        policy.packages.iter().map(|package| (package.name.as_str(), package)).collect();

    for package in &packages {
        let Some(expected) = policy_by_name.get(package.name.as_str()) else {
            diagnostics.push(Diagnostic::at(
                relative(root, &package.manifest_path),
                format!(
                    "workspace package `{}` has no architecture ownership record",
                    package.name
                ),
                "add a reviewed [[packages]] entry to architecture.toml",
            ));
            continue;
        };
        validate_package(root, policy, package, expected, &mut diagnostics)?;
    }
    for package in &policy.packages {
        if !cargo_by_name.contains_key(package.name.as_str()) {
            diagnostics.push(Diagnostic::at(
                &package.path,
                format!(
                    "policy registers `{}` but Cargo does not list it as a workspace member",
                    package.name
                ),
                "add the crate to workspace.members or remove the stale policy record",
            ));
        }
    }
    validate_dependency_edges(root, policy, &packages, &cargo_by_name, &mut diagnostics);
    Ok(diagnostics)
}

fn validate_package(
    root: &Path,
    policy: &ArchitecturePolicy,
    package: &CargoPackage,
    expected: &PackagePolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let manifest = relative(root, &package.manifest_path);
    let package_root = package.manifest_path.parent().unwrap_or_else(|| Path::new(""));
    let actual_path = relative(root, package_root);
    if actual_path != expected.path {
        diagnostics.push(Diagnostic::at(
            &manifest,
            format!("package path does not match registered path {}", expected.path.display()),
            "move the package or update architecture.toml through architecture review",
        ));
    }
    if package.license.as_deref() != Some(policy.required_license.as_str()) {
        diagnostics.push(Diagnostic::at(
            &manifest,
            format!("package must use workspace license `{}`", policy.required_license),
            "inherit license.workspace = true from the root manifest",
        ));
    }
    if package.edition != "2024" || package.rust_version.as_deref() != Some("1.97.1") {
        diagnostics.push(Diagnostic::at(
            &manifest,
            "package does not inherit the pinned edition and Rust version",
            "set edition.workspace = true and rust-version.workspace = true",
        ));
    }
    if package.version != "0.0.0" {
        diagnostics.push(Diagnostic::at(
            &manifest,
            "foundation package version must remain 0.0.0 before the release contract is established",
            "inherit version.workspace = true; versioning changes require release review",
        ));
    }
    let readme = package_readme_path(package);
    if readme.as_ref().is_none_or(|path| !path.exists()) {
        diagnostics.push(Diagnostic::at(
            &manifest,
            "package has no readable README",
            "add a README covering ownership, invariants, and dependency policy",
        ));
    }
    let package_toml = fs::read_to_string(&package.manifest_path)
        .map_err(|error| XtaskError::io("read", &package.manifest_path, error))?;
    let manifest_value: toml::Value = toml::from_str(&package_toml)
        .map_err(|error| XtaskError::parse_policy(&package.manifest_path, error))?;
    if manifest_value
        .get("lints")
        .and_then(|value| value.get("workspace"))
        .and_then(toml::Value::as_bool)
        != Some(true)
    {
        diagnostics.push(Diagnostic::at(
            &manifest,
            "package does not inherit workspace lints",
            "add [lints] workspace = true",
        ));
    }
    match &package.metadata.peritus {
        Some(actual)
            if actual.owner == expected.owner
                && actual.layer == expected.layer
                && actual.verification_class == expected.verification_class => {}
        Some(_) => diagnostics.push(Diagnostic::at(
            &manifest,
            "Cargo package ownership/layer/class metadata disagrees with architecture.toml",
            "make both reviewed records agree; do not silently change ownership",
        )),
        None => diagnostics.push(Diagnostic::at(
            &manifest,
            "Cargo package lacks [package.metadata.peritus]",
            "declare owner, layer, and verification-class",
        )),
    }
    Ok(())
}

fn package_readme_path(package: &CargoPackage) -> Option<PathBuf> {
    package.readme.as_ref().map(|path| {
        if path.is_absolute() {
            path.clone()
        } else {
            package.manifest_path.parent().unwrap_or_else(|| Path::new("")).join(path)
        }
    })
}

fn relative(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "architecture/verus_metadata_tests.rs"]
mod verus_metadata_tests;
