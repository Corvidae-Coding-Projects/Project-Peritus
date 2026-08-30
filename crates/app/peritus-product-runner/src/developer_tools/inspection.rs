//! Bounded read-only workspace inspection operations.

use std::{collections::VecDeque, fs, path::Path};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::{
    effect::limit,
    path::{checked, ignored, tool},
    wire::{bounded_usize, collection, object, required_string, string},
};
use crate::file_metadata;

const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;

pub(super) fn list(root: &Path, arguments: &Value) -> Result<Value, DeveloperLoopError> {
    let relative = string(arguments, "path").unwrap_or("");
    let depth = bounded_usize(arguments, "depth", 3, 1, 12);
    let start = if relative.is_empty() { root.to_owned() } else { checked(root, relative, false)? };
    let mut queue = VecDeque::from([(start, 0_usize)]);
    let mut entries = Vec::new();
    while let Some((directory, level)) = queue.pop_front() {
        let mut children = fs::read_dir(&directory)
            .map_err(|error| tool(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| tool(error.to_string()))?;
        children.sort_by_key(fs::DirEntry::file_name);
        for child in children {
            let path = child.path();
            let Some(relative) = path.strip_prefix(root).ok() else {
                continue;
            };
            if ignored(relative) {
                continue;
            }
            let kind = child.file_type().map_err(|error| tool(error.to_string()))?;
            let metadata = child.metadata().map_err(|error| tool(error.to_string()))?;
            entries.push(object(vec![
                ("path", Value::String(relative.to_string_lossy().into_owned())),
                (
                    "kind",
                    Value::String(if kind.is_dir() { "directory" } else { "file" }.to_owned()),
                ),
                ("bytes", Value::from(metadata.len())),
                ("permissions", Value::String(file_metadata::permissions(&metadata))),
            ]));
            if entries.len() >= 2_000 {
                return Ok(listing(root, entries, true));
            }
            if kind.is_dir() && level + 1 < depth {
                queue.push_back((path, level + 1));
            }
        }
    }
    Ok(listing(root, entries, false))
}

fn listing(root: &Path, entries: Vec<Value>, truncated: bool) -> Value {
    object(vec![
        ("workspace_root", Value::String(root.to_string_lossy().into_owned())),
        ("path_kind", Value::String("workspace-relative".to_owned())),
        ("entries", Value::Array(entries)),
        ("truncated", Value::Bool(truncated)),
    ])
}

pub(super) fn search(root: &Path, arguments: &Value) -> Result<Value, DeveloperLoopError> {
    let query = required_string(arguments, "query")?;
    if query.is_empty() {
        return Err(tool("search query is empty"));
    }
    let start = match string(arguments, "path") {
        Some(value) if !value.is_empty() => checked(root, value, false)?,
        _ => root.to_owned(),
    };
    let maximum = bounded_usize(arguments, "max_results", 200, 1, 1_000);
    let mut queue = VecDeque::from([start]);
    let mut matches = Vec::new();
    while let Some(path) = queue.pop_front() {
        let metadata = fs::symlink_metadata(&path).map_err(|error| tool(error.to_string()))?;
        if metadata.is_dir() {
            let mut children = fs::read_dir(path)
                .map_err(|error| tool(error.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| tool(error.to_string()))?;
            children.sort_by_key(fs::DirEntry::file_name);
            for child in children {
                if child.path().strip_prefix(root).is_ok_and(ignored) {
                    continue;
                }
                queue.push_back(child.path());
            }
            continue;
        }
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES as u64 {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            if line.contains(query) {
                matches.push(object(vec![
                    (
                        "path",
                        Value::String(
                            path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned(),
                        ),
                    ),
                    ("line", Value::from(index + 1)),
                    ("text", Value::String(line.to_owned())),
                ]));
                if matches.len() >= maximum {
                    return Ok(collection("matches", matches, true));
                }
            }
        }
    }
    Ok(collection("matches", matches, false))
}

pub(super) fn read(root: &Path, arguments: &Value) -> Result<Value, DeveloperLoopError> {
    let path = checked(root, required_string(arguments, "path")?, false)?;
    let metadata = fs::metadata(&path).map_err(|error| tool(error.to_string()))?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES as u64 {
        return Err(tool("file is not a bounded regular text file"));
    }
    let content = fs::read_to_string(&path).map_err(|error| tool(error.to_string()))?;
    let start = bounded_usize(arguments, "start_line", 1, 1, usize::MAX);
    let default_end = start.saturating_add(499);
    let end = bounded_usize(arguments, "end_line", default_end, start, usize::MAX);
    let lines = content
        .lines()
        .enumerate()
        .filter(|(index, _)| *index >= start.saturating_sub(1) && *index < end)
        .map(|(index, line)| format!("{}: {line}", index + 1))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(object(vec![
        ("content", Value::String(limit(&lines))),
        ("start_line", Value::from(start)),
        ("end_line", Value::from(end)),
        ("bytes", Value::from(metadata.len())),
        ("permissions", Value::String(file_metadata::permissions(&metadata))),
    ]))
}
