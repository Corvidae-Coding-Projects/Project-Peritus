//! Bounded read-only workspace inspection operations.

use std::{fs, path::Path};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::{
    inspection_cancellation::InspectionCancellation,
    inspection_read,
    path::{checked, ignored, tool},
    resources::CommandResources,
    wire::{bounded_usize, object, required_string},
};
use crate::file_metadata;

const MAX_PAGE_BYTES: usize = 512 * 1024;

pub(super) use super::inspection_list::list;

pub(super) fn listing(
    root: &Path,
    entries: Vec<Value>,
    omissions: &[Value],
    truncated: bool,
    next_cursor: Option<Value>,
    depth_limited: bool,
    resources: &CommandResources,
) -> Value {
    object(vec![
        ("workspace_root", Value::String(root.to_string_lossy().into_owned())),
        ("path_kind", Value::String("workspace-relative".to_owned())),
        ("execution_resources", resources.observation()),
        ("entries", Value::Array(entries)),
        ("omissions", Value::Array(omissions.to_vec())),
        ("truncated", Value::Bool(truncated)),
        ("next_cursor", next_cursor.unwrap_or(Value::Null)),
        ("depth_limited", Value::Bool(depth_limited)),
    ])
}

pub(super) fn read(
    root: &Path,
    arguments: &Value,
    cancellation: &InspectionCancellation,
) -> Result<Value, DeveloperLoopError> {
    cancellation.check()?;
    let path = checked(root, required_string(arguments, "path")?, true)?;
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            tool("not_found: this file does not exist. Use the observed workspace listing; do not repeat this read unless the file has since been created. For an authorized greenfield task, create the planned file with workspace_write rather than assuming a manifest already exists.")
        } else { tool(error.to_string()) }
    })?;
    if !metadata.is_file() {
        return Err(tool("path is not a regular text file"));
    }
    let extra = vec![
        ("bytes", Value::from(metadata.len())),
        ("permissions", Value::String(file_metadata::permissions(&metadata))),
    ];
    inspection_read::read(&path, arguments, &extra, cancellation)
}

pub(super) fn entry_kind(kind: fs::FileType) -> &'static str {
    if kind.is_dir() {
        "directory"
    } else if kind.is_file() {
        "file"
    } else if kind.is_symlink() {
        "symlink"
    } else {
        "other"
    }
}

pub(super) fn page_bytes(arguments: &Value) -> usize {
    bounded_usize(arguments, "max_bytes", MAX_PAGE_BYTES, 256, MAX_PAGE_BYTES)
}

pub(super) fn ignored_from(start: &Path, candidate: &Path) -> bool {
    if start.as_os_str().is_empty() {
        ignored(candidate)
    } else {
        candidate.strip_prefix(start).map_or_else(|_| ignored(candidate), ignored)
    }
}

pub(super) fn omission(path: &Path, error: impl std::fmt::Display) -> Value {
    object(vec![
        ("path", Value::String(path.to_string_lossy().into_owned())),
        ("error", Value::String(error.to_string())),
    ])
}

pub(super) fn encoded_len(value: &Value) -> Result<usize, DeveloperLoopError> {
    serde_json::to_vec(value).map(|bytes| bytes.len()).map_err(|error| tool(error.to_string()))
}
