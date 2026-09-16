//! Conservative ownership restriction for manifest-relative embedded data.
//!
//! Cargo supplies `CARGO_MANIFEST_DIR` from the compiling package. Physical source ownership
//! identifies that package only when roots are disjoint, targets remain inside their package,
//! and literal source references cannot leave it. We enforce that restriction before resolving
//! any manifest-relative data operand. This is a local source policy, not a Rust/Verus limitation.
//! It intentionally rejects cross-package source sharing even when its configurations might
//! avoid compiling the manifest-relative expression. Ordinary downward modules remain inside
//! the disjoint package roots; static references are checked at the source file's directory,
//! before any extra directory prefix contributed by an inline module.

use super::manifest_directory;
use crate::error::XtaskError;
use crate::model::CargoMetadata;
use crate::source::static_compilation_references;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub(super) fn check(
    root: &Path,
    rust_sources: &[PathBuf],
    cargo: &CargoMetadata,
) -> Result<(), XtaskError> {
    let packages: Vec<_> = cargo
        .packages
        .iter()
        .filter(|package| cargo.workspace_members.contains(&package.id))
        .collect();
    let directories: Vec<_> = packages
        .iter()
        .map(|package| {
            package.manifest_path.parent().ok_or_else(|| invalid("package has no directory"))
        })
        .collect::<Result<_, _>>()?;
    for (index, directory) in directories.iter().enumerate() {
        if directories
            .iter()
            .enumerate()
            .any(|(other, candidate)| index != other && directory.starts_with(candidate))
        {
            return Err(invalid(format!(
                "nested or duplicate package directory {} makes default-module ownership ambiguous",
                directory.display()
            )));
        }
        for target in &packages[index].targets {
            let target_path = normalized(root, &target.src_path)?;
            if manifest_directory(&target_path, cargo) != Some(*directory) {
                return Err(invalid(format!(
                    "Cargo target {} is outside its compiling package {}",
                    target_path.display(),
                    packages[index].name
                )));
            }
        }
    }
    for source in rust_sources {
        let contents = fs::read_to_string(source)
            .map_err(|error| XtaskError::io("read source context", source, error))?;
        for (declared, line, kind) in static_compilation_references(&contents) {
            let declared = declared.ok_or_else(|| {
                invalid(format!(
                    "{}:{line} has an unsupported {kind} source operand",
                    source.display()
                ))
            })?;
            let directory = source.parent().ok_or_else(|| invalid("source has no parent"))?;
            let target = normalized(root, &directory.join(declared))?;
            let owner = manifest_directory(source, cargo);
            if owner.is_none() || manifest_directory(&target, cargo) != owner {
                return Err(invalid(format!(
                    "{}:{line} {kind} crosses a package source boundary to {}",
                    source.display(),
                    target.display()
                )));
            }
        }
    }
    Ok(())
}

fn normalized(root: &Path, path: &Path) -> Result<PathBuf, XtaskError> {
    let absolute = if path.is_absolute() { path.to_path_buf() } else { root.join(path) };
    let mut result = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            _ => result.push(component.as_os_str()),
        }
    }
    if !result.starts_with(root) {
        return Err(invalid("compilation source leaves the workspace"));
    }
    Ok(result)
}

fn invalid(message: impl AsRef<str>) -> XtaskError {
    XtaskError::metadata(format!(
        "CARGO_MANIFEST_DIR data requires disjoint package-local compilation sources: {}",
        message.as_ref()
    ))
}
