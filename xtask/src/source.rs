use crate::error::{Diagnostic, ErrorCode, XtaskError};
use crate::model::{ArchitecturePolicy, CargoMetadata, ControlledSourceKind, PackagePolicy};
use std::collections::BTreeSet;
use std::fs::{self, DirEntry};
use std::path::{Component, Path, PathBuf};

mod crate_root;
mod layout_discovery;
mod reference;
pub(crate) mod reference_lexer;
mod target_root;
mod trust_discovery;

use crate_root::inspect_crate_root;

pub(crate) fn check(
    root: &Path,
    policy: &ArchitecturePolicy,
    cargo: &CargoMetadata,
) -> Result<usize, XtaskError> {
    let discovery = layout_discovery::discover(root, policy, cargo)?;
    let mut diagnostics = discovery.diagnostics;
    let mut active_exceptions = BTreeSet::new();
    for file in &discovery.files {
        inspect_file(root, file, policy, &mut active_exceptions, &mut diagnostics)?;
    }
    inspect_controlled_sources(root, policy, &mut diagnostics)?;
    for exception in &policy.source_exceptions {
        if !active_exceptions.contains(&exception.path) {
            diagnostics.push(Diagnostic::at(
                &exception.path,
                "source-size exception is stale because the file is within the normal budget",
                "remove the exception so oversized-source debt remains visible and accurate",
            ));
        }
    }

    if diagnostics.is_empty() {
        Ok(discovery.files.len())
    } else {
        Err(XtaskError::violations(ErrorCode::SourceLayout, "source-layout-check", diagnostics))
    }
}

pub(crate) fn collect_rust_files(
    root: &Path,
    policy: &ArchitecturePolicy,
) -> Result<Vec<PathBuf>, XtaskError> {
    let mut files = Vec::new();
    visit(root, root, policy, &mut files)?;
    files.sort();
    Ok(files)
}

pub(crate) fn discover_compilation_sources(
    root: &Path,
    policy: &ArchitecturePolicy,
    target_roots: &[PathBuf],
) -> Result<trust_discovery::Discovery, XtaskError> {
    trust_discovery::discover(root, policy, target_roots)
}

fn visit(
    root: &Path,
    directory: &Path,
    policy: &ArchitecturePolicy,
    files: &mut Vec<PathBuf>,
) -> Result<(), XtaskError> {
    let mut entries: Vec<DirEntry> = fs::read_dir(directory)
        .map_err(|error| XtaskError::io("read directory", directory, error))?
        .collect::<Result<_, _>>()
        .map_err(|error| XtaskError::io("read directory entry in", directory, error))?;
    entries.sort_by_key(DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);
        if is_ignored(relative, &policy.ignored_directories) {
            continue;
        }
        let file_type =
            entry.file_type().map_err(|error| XtaskError::io("inspect", &path, error))?;
        if file_type.is_symlink() {
            let architecture_owned = owning_package(relative, &policy.packages).is_some();
            if architecture_owned {
                files.push(path);
            }
        } else if file_type.is_dir() {
            visit(root, &path, policy, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && (owning_package(relative, &policy.packages).is_some()
                || relative.starts_with("crates"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn inspect_controlled_sources(
    root: &Path,
    policy: &ArchitecturePolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let mut entries: Vec<DirEntry> = fs::read_dir(&directory)
            .map_err(|error| XtaskError::io("read directory", &directory, error))?
            .collect::<Result<_, _>>()
            .map_err(|error| XtaskError::io("read directory entry in", &directory, error))?;
        entries.sort_by_key(DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap_or(&path);
            if is_ignored(relative, &policy.ignored_directories) {
                continue;
            }
            let file_type =
                entry.file_type().map_err(|error| XtaskError::io("inspect", &path, error))?;
            if file_type.is_dir() {
                pending.push(path);
            } else {
                inspect_controlled_path(relative, file_type.is_symlink(), policy, diagnostics);
            }
        }
    }
    Ok(())
}

fn inspect_controlled_path(
    relative: &Path,
    is_symlink: bool,
    policy: &ArchitecturePolicy,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(kind) = controlled_kind(relative) else { return };
    let owner = policy
        .controlled_source_roots
        .iter()
        .filter(|controlled| relative.starts_with(&controlled.path))
        .filter(|controlled| controlled_kind_covers(controlled.kind, kind))
        .max_by_key(|controlled| controlled.path.components().count());
    let Some(owner) = owner else {
        diagnostics.push(Diagnostic::at(
            relative,
            "generated or schema source has no reviewed ownership root",
            "add the narrowest controlled_source_roots entry with owner, kind, and rationale",
        ));
        return;
    };
    if is_symlink {
        diagnostics.push(Diagnostic::at(
            relative,
            "controlled generated/schema source is a symbolic link",
            "check in the reviewed source directly so ownership checks cannot be redirected",
        ));
    }
    if let Some(package) = owning_package(relative, &policy.packages)
        && package.owner != owner.owner
    {
        diagnostics.push(Diagnostic::at(
            relative,
            format!(
                "controlled source owner `{}` disagrees with package owner `{}`",
                owner.owner, package.owner
            ),
            "assign the controlled root to the package owner or move it to its owning boundary",
        ));
    }
}

pub(super) fn controlled_kind(path: &Path) -> Option<ControlledSourceKind> {
    let mut generated = false;
    let mut schema = false;
    for component in path.components().filter_map(|component| component.as_os_str().to_str()) {
        generated |= component == "generated";
        schema |= matches!(component, "schema" | "schemas");
    }
    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
        let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or(name);
        generated |= name.contains(".generated.")
            || stem.ends_with("_generated")
            || stem.starts_with("generated_");
        schema |= name.contains(".schema.") || stem == "schema" || stem.ends_with("_schema");
    }
    schema |= path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "capnp" | "jsonschema" | "proto" | "wit"));
    match (generated, schema) {
        (true, true) => Some(ControlledSourceKind::GeneratedSchema),
        (true, false) => Some(ControlledSourceKind::Generated),
        (false, true) => Some(ControlledSourceKind::Schema),
        (false, false) => None,
    }
}

pub(super) fn controlled_kind_covers(
    owner: ControlledSourceKind,
    actual: ControlledSourceKind,
) -> bool {
    owner == actual || matches!(owner, ControlledSourceKind::GeneratedSchema)
}

fn inspect_file(
    root: &Path,
    file: &layout_discovery::LayoutFile,
    policy: &ArchitecturePolicy,
    active_exceptions: &mut BTreeSet<PathBuf>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let relative = file.path.strip_prefix(root).unwrap_or(&file.path);
    let metadata = fs::symlink_metadata(&file.path)
        .map_err(|error| XtaskError::io("inspect", &file.path, error))?;
    if metadata.file_type().is_symlink() {
        diagnostics.push(Diagnostic::at(
            relative,
            "Rust source path is a symbolic link",
            "replace it with reviewed repository-owned source to prevent policy bypass",
        ));
        return Ok(());
    }
    if file.package.is_none() {
        diagnostics.push(Diagnostic::at(
            relative,
            "compilation source has no registered package owner",
            "register its Cargo package and owning slice in architecture.toml",
        ));
        return Ok(());
    }
    let contents = fs::read_to_string(&file.path)
        .map_err(|error| XtaskError::io("read", &file.path, error))?;
    let line_count = contents.lines().count();
    let exception = policy.source_exceptions.iter().find(|candidate| candidate.path == relative);
    if line_count > policy.soft_source_lines {
        if exception.is_some() {
            active_exceptions.insert(relative.to_path_buf());
        } else {
            let level = if line_count > policy.hard_source_lines { "hard" } else { "soft" };
            diagnostics.push(Diagnostic::at(
                relative,
                format!(
                    "source has {line_count} lines and exceeds the {level} budget of {}",
                    if level == "hard" {
                        policy.hard_source_lines
                    } else {
                        policy.soft_source_lines
                    }
                ),
                "split responsibilities or add a reviewed exception with owner and rationale",
            ));
        }
    }
    if let Some(stem) = file.path.file_stem().and_then(|name| name.to_str())
        && policy.forbidden_module_names.iter().any(|forbidden| forbidden == stem)
    {
        diagnostics.push(Diagnostic::at(
            relative,
            format!("generic module name `{stem}` is prohibited"),
            "rename the module for its domain responsibility and document that boundary",
        ));
    }
    for pattern in [["to", "do!"].concat(), ["un", "implemented!"].concat()] {
        if contents.contains(&pattern) {
            diagnostics.push(Diagnostic::at(
                relative,
                format!("reachable placeholder macro `{pattern}` is prohibited"),
                "implement a typed failure or complete behavior before merging",
            ));
        }
    }
    if let Some(root_kind) = file.root_kind {
        inspect_crate_root(relative, &contents, policy.root_module_lines, root_kind, diagnostics);
    }
    Ok(())
}

fn owning_package<'a>(relative: &Path, packages: &'a [PackagePolicy]) -> Option<&'a PackagePolicy> {
    packages
        .iter()
        .filter(|package| relative.starts_with(&package.path))
        .max_by_key(|package| package.path.components().count())
}

pub(super) fn is_ignored(relative: &Path, ignored: &[String]) -> bool {
    ignored.iter().map(Path::new).any(|prefix| {
        !prefix.as_os_str().is_empty()
            && prefix.components().all(|component| matches!(component, Component::Normal(_)))
            && relative.starts_with(prefix)
    })
}

#[cfg(test)]
mod tests;
