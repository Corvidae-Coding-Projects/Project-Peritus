use crate::error::{Diagnostic, XtaskError};
use crate::model::CargoMetadata;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn validate(
    root: &Path,
    cargo: &CargoMetadata,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    for path in manifest_paths(root, cargo) {
        let contents =
            fs::read_to_string(&path).map_err(|error| XtaskError::io("read", &path, error))?;
        let manifest: toml::Value =
            toml::from_str(&contents).map_err(|error| XtaskError::parse_policy(&path, error))?;
        for table in ["patch", "replace"] {
            if manifest.get(table).is_some() {
                diagnostics.push(Diagnostic::at(
                    relative(root, &path),
                    format!("Cargo [{table}] dependency overrides are forbidden in A0 manifests"),
                    "remove registry, Git, and path overrides so Cargo.lock and metadata identify the code that builds",
                ));
            }
        }
    }
    Ok(())
}

fn manifest_paths(root: &Path, cargo: &CargoMetadata) -> BTreeSet<PathBuf> {
    std::iter::once(root.join("Cargo.toml"))
        .chain(
            cargo
                .packages
                .iter()
                .filter(|package| cargo.workspace_members.contains(&package.id))
                .map(|package| package.manifest_path.clone()),
        )
        .collect()
}

fn relative<'a>(root: &Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}
