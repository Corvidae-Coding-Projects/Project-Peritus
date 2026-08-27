use crate::error::XtaskError;
use crate::metadata;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

pub(crate) fn check(root: &Path) -> Result<usize, XtaskError> {
    let cargo = metadata::cargo_metadata(root)?;
    let workspace_members = cargo.workspace_members.into_iter().collect::<BTreeSet<_>>();
    let mut package_names = cargo
        .packages
        .into_iter()
        .filter(|package| workspace_members.contains(&package.id))
        .map(|package| package.name)
        .collect::<Vec<_>>();
    package_names.sort_unstable();

    for package_name in &package_names {
        let status = Command::new("cargo")
            .args(["fmt", "--package"])
            .arg(package_name)
            .args(["--", "--check"])
            .current_dir(root)
            .status()
            .map_err(|error| XtaskError::io("execute cargo fmt in", root, error))?;
        if !status.success() {
            return Err(XtaskError::formatting(format!(
                "cargo fmt failed for workspace package `{package_name}` with status {status}"
            )));
        }
    }

    Ok(package_names.len())
}
