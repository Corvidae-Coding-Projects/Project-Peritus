//! Read-only inspection of absolute paths explicitly named by the user's task.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::{
    executor::WorkspaceDeveloperTools,
    inspection_cancellation::InspectionCancellation,
    inspection_read,
    path::tool,
    reference_path::{
        case_correct_descendant, case_correct_path, explicit_absolute_path,
        path_starts_with_native, task_tokens,
    },
    wire::required_string,
};
use crate::file_metadata;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ExplicitReferences {
    roots: Vec<PathBuf>,
}

impl ExplicitReferences {
    fn from_task(workspace_root: &Path, task: &str) -> Self {
        let mut roots = BTreeSet::new();
        for token in task_tokens(task) {
            let Some(path) = explicit_absolute_path(&token.0, token.1) else { continue };
            if path_starts_with_native(&path, workspace_root) {
                continue;
            }
            roots.insert(path);
        }
        Self { roots: roots.into_iter().collect() }
    }

    pub(super) fn resolve(&self, raw: &str) -> Result<(PathBuf, PathBuf), DeveloperLoopError> {
        let requested = explicit_absolute_path(raw, true)
            .ok_or_else(|| tool("reference path must be a normal absolute path"))?;
        let root = self
            .roots
            .iter()
            .filter(|root| path_starts_with_native(&requested, root))
            .max_by_key(|root| root.components().count())
            .cloned()
            .ok_or_else(|| {
                tool("reference path was not explicitly named by the user's task; no read authority was granted")
            })?;
        let root_components = root.components().count();
        let resolved_root = case_correct_path(&root)?;
        let resolved_requested =
            case_correct_descendant(&resolved_root, &requested, root_components)?;
        Ok((resolved_root, resolved_requested))
    }
}

impl WorkspaceDeveloperTools {
    #[must_use]
    pub(crate) fn with_reference_contract(mut self, task: &str) -> Self {
        self.references = ExplicitReferences::from_task(&self.root, task);
        self
    }
}

pub(super) use super::reference_list::list;

pub(super) fn read(
    references: &ExplicitReferences,
    arguments: &Value,
    cancellation: &InspectionCancellation,
) -> Result<Value, DeveloperLoopError> {
    cancellation.check()?;
    let (root, path) = references.resolve(required_string(arguments, "path")?)?;
    let metadata = fs::symlink_metadata(&path).map_err(|error| reference_io_error(&error))?;
    if !metadata.is_file() {
        return Err(tool("external reference path is not a regular text file"));
    }
    let extra = vec![
        ("reference_root", Value::String(root.to_string_lossy().into_owned())),
        ("path", Value::String(path.to_string_lossy().into_owned())),
        ("bytes", Value::from(metadata.len())),
        ("permissions", Value::String(file_metadata::permissions(&metadata))),
    ];
    inspection_read::read(&path, arguments, &extra, cancellation)
}

pub(super) fn reference_io_error(error: &std::io::Error) -> DeveloperLoopError {
    if error.kind() == std::io::ErrorKind::NotFound {
        reference_not_found()
    } else {
        tool(error.to_string())
    }
}

pub(super) fn reference_not_found() -> DeveloperLoopError {
    tool("not_found: no exact or filesystem-native alias exists for the explicit reference path")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_paths_are_exact_bounded_and_outside_the_workspace() {
        let workspace = Path::new("/work/output");
        let workspace_source = workspace.join("src");
        let references = ExplicitReferences::from_task(
            workspace,
            &format!(
                "match '/reference files/invoices' and '{}' while ignoring https://host/a",
                workspace_source.display()
            ),
        );
        assert!(references.roots.iter().all(|root| !root.starts_with(workspace)));
        #[cfg(unix)]
        assert_eq!(references.roots, vec![PathBuf::from("/reference files/invoices")]);
    }
}
