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
    pub(super) fn from_task(workspace_root: &Path, task: &str) -> Self {
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
        self.rust_reliability.task(task);
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
    let before = super::grounding::file_digest(&path).map_err(|error| tool(error.to_string()))?;
    let extra = vec![
        ("source_sha256", Value::String(before.clone())),
        ("reference_root", Value::String(root.to_string_lossy().into_owned())),
        ("path", Value::String(path.to_string_lossy().into_owned())),
        ("bytes", Value::from(metadata.len())),
        ("permissions", Value::String(file_metadata::permissions(&metadata))),
    ];
    let value = inspection_read::read(&path, arguments, &extra, cancellation)?;
    if super::grounding::file_digest(&path).map_err(|error| tool(error.to_string()))? != before {
        return Err(tool("reference changed during observation; refresh only this reference"));
    }
    Ok(value)
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

/// Revalidates reference authority using the current literal user contract, never archived text.
pub(crate) fn authorized_file(
    workspace: &Path,
    contract: &str,
    raw: &str,
) -> Result<PathBuf, DeveloperLoopError> {
    ExplicitReferences::from_task(workspace, contract).resolve(raw).map(|(_, file)| file)
}
