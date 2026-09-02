//! Canonical containment and portable relative paths for retained benchmark reports.

use std::fs;
use std::path::{Component, Path};

use crate::BenchmarkError;

pub fn canonical_relative(
    root: &Path,
    path: &Path,
    label: &'static str,
) -> Result<String, BenchmarkError> {
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| BenchmarkError::filesystem("canonicalize report root", root, error))?;
    let canonical_path = fs::canonicalize(path)
        .map_err(|error| BenchmarkError::filesystem("canonicalize report evidence", path, error))?;
    let relative = canonical_path.strip_prefix(&canonical_root).map_err(|_| {
        BenchmarkError::Workspace(format!("{label} escaped its report root: {}", path.display()))
    })?;
    portable(relative, label)
}

pub fn join(relative: &str, suffix: &'static str) -> String {
    format!("{relative}/{suffix}")
}

pub fn validate(value: &str, label: &'static str) -> Result<(), BenchmarkError> {
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value.split('/').any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        return Err(BenchmarkError::Workspace(format!(
            "{label} is not a portable relative path: {value:?}"
        )));
    }
    Ok(())
}

fn portable(path: &Path, label: &'static str) -> Result<String, BenchmarkError> {
    let mut output = String::new();
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(BenchmarkError::Workspace(format!(
                "{label} is not a normal relative path: {}",
                path.display()
            )));
        };
        let component = component.to_str().ok_or_else(|| {
            BenchmarkError::Workspace(format!("{label} contains non-UTF-8 path text"))
        })?;
        if component.contains(['/', '\\']) {
            return Err(BenchmarkError::Workspace(format!(
                "{label} contains a non-portable path component"
            )));
        }
        if !output.is_empty() {
            output.push('/');
        }
        output.push_str(component);
    }
    validate(&output, label)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_validation_rejects_absolute_parent_and_windows_separators() {
        for invalid in ["", "/root/file", "../file", "dir/../file", "dir\\file"] {
            assert!(validate(invalid, "fixture path").is_err(), "accepted {invalid:?}");
        }
        validate("trial/result.json", "fixture path").expect("portable path");
    }

    #[cfg(unix)]
    #[test]
    fn canonical_containment_normalizes_root_aliases_before_comparison() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().expect("temporary root");
        let root = temporary.path().join("canonical-campaign");
        let evidence = root.join("workspaces/task/invocation.json");
        fs::create_dir_all(evidence.parent().expect("evidence parent")).expect("directories");
        fs::write(&evidence, b"{}").expect("evidence");
        let alias = temporary.path().join("campaign-alias");
        symlink(&root, &alias).expect("root alias");

        let relative = canonical_relative(
            &root,
            &alias.join("workspaces/task/invocation.json"),
            "fixture evidence",
        )
        .expect("canonical containment");
        assert_eq!(relative, "workspaces/task/invocation.json");
    }
}
