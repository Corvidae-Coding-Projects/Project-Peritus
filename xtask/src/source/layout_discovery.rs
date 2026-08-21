//! Owner-attributed source-layout discovery.
//!
//! Repository Rust files retain the legacy unowned-source diagnostic, but only sources attributed
//! to a workspace target's registered package are followed as compilation inputs. Static inputs
//! inherit that originating package only while they remain inside its reviewed physical boundary.

use super::crate_root::RootKind;
use super::reference::SourceReference;
use crate::error::{Diagnostic, XtaskError};
use crate::model::{ArchitecturePolicy, CargoMetadata, CargoTarget};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) struct LayoutFile {
    pub(super) path: PathBuf,
    pub(super) package: Option<usize>,
    pub(super) root_kind: Option<RootKind>,
}

pub(super) struct Discovery {
    pub(super) files: Vec<LayoutFile>,
    pub(super) diagnostics: Vec<Diagnostic>,
}

pub(super) fn discover(
    root: &Path,
    policy: &ArchitecturePolicy,
    cargo: &CargoMetadata,
) -> Result<Discovery, XtaskError> {
    let mut files = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for path in super::collect_rust_files(root, policy)? {
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let package = owning_package_index(relative, policy);
        files.insert(path.clone(), LayoutFile { path, package, root_kind: None });
    }
    add_target_roots(root, policy, cargo, &mut files, &mut diagnostics);
    follow_references(root, policy, &mut files, &mut diagnostics)?;
    Ok(Discovery { files: files.into_values().collect(), diagnostics })
}

fn add_target_roots(
    root: &Path,
    policy: &ArchitecturePolicy,
    cargo: &CargoMetadata,
    files: &mut BTreeMap<PathBuf, LayoutFile>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let workspace_ids: BTreeSet<_> = cargo.workspace_members.iter().map(String::as_str).collect();
    for package in
        cargo.packages.iter().filter(|package| workspace_ids.contains(package.id.as_str()))
    {
        let package_index = policy.packages.iter().position(|owned| owned.name == package.name);
        for target in &package.targets {
            let Some(path) =
                super::target_root::validate(root, &target.src_path, policy, diagnostics)
            else {
                continue;
            };
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let Some(package_index) = package_index else {
                diagnostics.push(Diagnostic::at(
                    relative,
                    format!("Cargo target for `{}` has no registered package owner", package.name),
                    "add the package to architecture.toml before compiling any target source",
                ));
                continue;
            };
            let owner = &policy.packages[package_index];
            if owning_package_index(relative, policy) != Some(package_index) {
                diagnostics.push(Diagnostic::at(
                    relative,
                    format!(
                        "Cargo target for `{}` is outside its registered package path `{}`",
                        package.name,
                        owner.path.display()
                    ),
                    "move the target source below its owning package or register the correct package boundary",
                ));
                continue;
            }
            let root_kind = composition_root_kind(target);
            files
                .entry(path.clone())
                .and_modify(|file| {
                    file.package = Some(package_index);
                    file.root_kind = file.root_kind.or(root_kind);
                })
                .or_insert(LayoutFile { path, package: Some(package_index), root_kind });
        }
    }
}

fn follow_references(
    root: &Path,
    policy: &ArchitecturePolicy,
    files: &mut BTreeMap<PathBuf, LayoutFile>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let mut pending: VecDeque<PathBuf> = files
        .values()
        .filter(|file| file.package.is_some())
        .map(|file| file.path.clone())
        .collect();
    let mut inspected = BTreeSet::new();
    while let Some(path) = pending.pop_front() {
        if !inspected.insert(path.clone()) {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let package_index = files[&path].package.expect("only owned sources are queued");
        let bytes = fs::read(&path).map_err(|error| XtaskError::io("read", &path, error))?;
        let Ok(contents) = String::from_utf8(bytes) else {
            diagnostics.push(Diagnostic::at(
                relative,
                "compilation source is not valid UTF-8 and cannot be layout-checked",
                "store repository compilation inputs as UTF-8 Rust source",
            ));
            continue;
        };
        for reference in super::reference::scan(&contents).references {
            if let Some((target, file)) = resolve_owned_reference(
                root,
                relative,
                package_index,
                &reference,
                policy,
                diagnostics,
            ) && let std::collections::btree_map::Entry::Vacant(entry) =
                files.entry(target.clone())
            {
                entry.insert(file);
                pending.push_back(target);
            }
        }
    }
    Ok(())
}

fn resolve_owned_reference(
    root: &Path,
    source: &Path,
    package_index: usize,
    reference: &SourceReference,
    policy: &ArchitecturePolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(PathBuf, LayoutFile)> {
    let path =
        super::trust_discovery::validate_reference(root, source, reference, policy, diagnostics)?;
    let relative = path.strip_prefix(root).unwrap_or(&path);
    let owner = &policy.packages[package_index];
    if owning_package_index(relative, policy) != Some(package_index) {
        diagnostics.push(Diagnostic::at(
            source,
            format!(
                "line {} {} source `{}` is outside originating package `{}`",
                reference.line,
                reference.kind.label(),
                relative.display(),
                owner.name
            ),
            "move the input below the originating package so compiled source has one explicit owner",
        ));
        return None;
    }
    let file = LayoutFile { path: path.clone(), package: Some(package_index), root_kind: None };
    Some((path, file))
}

fn owning_package_index(relative: &Path, policy: &ArchitecturePolicy) -> Option<usize> {
    policy
        .packages
        .iter()
        .enumerate()
        .filter(|(_, package)| relative.starts_with(&package.path))
        .max_by_key(|(_, package)| package.path.components().count())
        .map(|(index, _)| index)
}

fn composition_root_kind(target: &CargoTarget) -> Option<RootKind> {
    if target.kind.iter().any(|kind| kind == "bin") {
        Some(RootKind::Binary)
    } else if target
        .kind
        .iter()
        .any(|kind| matches!(kind.as_str(), "lib" | "rlib" | "dylib" | "staticlib" | "cdylib"))
    {
        Some(RootKind::Library)
    } else {
        None
    }
}
