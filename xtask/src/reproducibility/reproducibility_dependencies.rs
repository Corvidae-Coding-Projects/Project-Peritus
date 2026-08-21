use super::has_full_git_revision;
use crate::error::Diagnostic;
use crate::model::{CargoMetadata, CargoPackage};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

pub(super) fn validate(root: &Path, cargo: &CargoMetadata, diagnostics: &mut Vec<Diagnostic>) {
    let Ok(canonical_root) = root.canonicalize() else {
        diagnostics.push(Diagnostic::at(
            root,
            "workspace root cannot be canonicalized for path-dependency policy",
            "restore the workspace root as a readable regular directory",
        ));
        return;
    };
    let members = member_roots(cargo);
    for package in &cargo.packages {
        for dependency in &package.dependencies {
            if let Some(path) = &dependency.path {
                validate_path(
                    root,
                    &canonical_root,
                    package,
                    &dependency.name,
                    path,
                    &members,
                    diagnostics,
                );
                continue;
            }
            let reproducible =
                if dependency.source.as_deref().is_some_and(|source| source.starts_with("git+")) {
                    dependency.source.as_deref().is_some_and(has_full_git_revision)
                } else {
                    is_exact_registry_requirement(&dependency.req)
                };
            if !reproducible {
                diagnostics.push(Diagnostic::at(
                    relative(root, &package.manifest_path),
                    format!(
                        "dependency `{}` uses non-exact requirement `{}` from {:?}",
                        dependency.name, dependency.req, dependency.source
                    ),
                    "pin registry dependencies with one complete =MAJOR.MINOR.PATCH requirement (optionally a valid prerelease) and Git dependencies with a full commit rev",
                ));
            }
        }
    }
}

pub(super) fn is_exact_registry_requirement(requirement: &str) -> bool {
    let Some(version) = requirement.strip_prefix('=') else { return false };
    if version.is_empty()
        || version.contains('+')
        || version.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return false;
    }
    let (core, prerelease) = version
        .split_once('-')
        .map_or((version, None), |(core, prerelease)| (core, Some(prerelease)));
    let mut components = core.split('.');
    let complete_core = components.next().is_some_and(valid_numeric_identifier)
        && components.next().is_some_and(valid_numeric_identifier)
        && components.next().is_some_and(valid_numeric_identifier)
        && components.next().is_none();
    complete_core
        && prerelease.is_none_or(|value| {
            !value.is_empty()
                && value.split('.').all(|identifier| {
                    !identifier.is_empty()
                        && identifier
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                        && (!identifier.bytes().all(|byte| byte.is_ascii_digit())
                            || valid_numeric_identifier(identifier))
                })
        })
}

fn valid_numeric_identifier(identifier: &str) -> bool {
    !identifier.is_empty()
        && identifier.bytes().all(|byte| byte.is_ascii_digit())
        && (identifier == "0" || !identifier.starts_with('0'))
}

fn member_roots(cargo: &CargoMetadata) -> BTreeSet<PathBuf> {
    cargo
        .packages
        .iter()
        .filter(|package| cargo.workspace_members.contains(&package.id))
        .filter_map(|package| package.manifest_path.parent())
        .filter_map(|path| path.canonicalize().ok())
        .collect()
}

fn validate_path(
    root: &Path,
    canonical_root: &Path,
    package: &CargoPackage,
    name: &str,
    declared: &Path,
    members: &BTreeSet<PathBuf>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let base = package.manifest_path.parent().unwrap_or(root);
    let resolved =
        if declared.is_absolute() { declared.to_path_buf() } else { base.join(declared) };
    let lexical_relative = resolved.strip_prefix(root).ok();
    let safe_lexical = lexical_relative.is_some_and(|relative| {
        relative.components().all(|component| matches!(component, Component::Normal(_)))
            && !has_symlink_component(root, relative)
    });
    let canonical = resolved.canonicalize().ok();
    let inside = canonical.as_deref().is_some_and(|path| path.starts_with(canonical_root));
    let registered = canonical.as_ref().is_some_and(|path| members.contains(path));
    if !safe_lexical || !inside || !registered {
        diagnostics.push(Diagnostic::at(
            relative(root, &package.manifest_path),
            format!(
                "path dependency `{name}` at `{}` is not a direct registered workspace member",
                declared.display()
            ),
            "use a non-symlink path inside the repository whose package root is listed in workspace_members",
        ));
    }
}

fn has_symlink_component(root: &Path, relative: &Path) -> bool {
    let mut current = root.to_path_buf();
    relative.components().any(|component| {
        current.push(component);
        current.symlink_metadata().is_ok_and(|metadata| metadata.file_type().is_symlink())
    })
}

fn relative<'a>(root: &Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}
