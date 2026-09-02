//! Exact, non-recursive workspace removal behavior.

use std::{fs, path::Path};

use peritus_agent::DeveloperLoopError;
use serde_json::{Map, Value};

use super::{
    grounding::GroundingEvidence,
    ownership::WorkspaceOwnership,
    path::{checked, tool},
};

pub(super) fn remove(
    root: &Path,
    grounding: &GroundingEvidence,
    ownership: &WorkspaceOwnership,
    arguments: &Value,
) -> Result<Value, DeveloperLoopError> {
    let relative =
        arguments.get("path").and_then(Value::as_str).ok_or_else(|| tool("path must be text"))?;
    if relative.is_empty() || relative == "." {
        return Err(tool("workspace_remove cannot remove the workspace root"));
    }
    let path = checked(root, relative, false)?;
    let metadata = fs::metadata(&path).map_err(|error| tool(error.to_string()))?;
    if metadata.is_dir() {
        grounding.ensure_empty_directory_removal_allowed(relative).map_err(tool)?;
        if fs::read_dir(&path)
            .map_err(|error| tool(error.to_string()))?
            .next()
            .transpose()
            .map_err(|error| tool(error.to_string()))?
            .is_some()
        {
            return Err(tool("workspace_remove only removes an empty directory"));
        }
        fs::remove_dir(&path).map_err(|error| tool(error.to_string()))?;
        return Ok(result(relative, "directory"));
    }
    if !metadata.is_file() {
        return Err(tool("workspace_remove requires one regular file or empty directory"));
    }
    grounding.ensure_mutation_allowed(relative, true).map_err(tool)?;
    ownership.ensure_removable(&path)?;
    fs::remove_file(&path).map_err(|error| tool(error.to_string()))?;
    Ok(result(relative, "file"))
}

fn result(path: &str, kind: &str) -> Value {
    Value::Object(
        [
            ("path".to_owned(), Value::String(path.to_owned())),
            ("kind".to_owned(), Value::String(kind.to_owned())),
        ]
        .into_iter()
        .collect::<Map<_, _>>(),
    )
}
