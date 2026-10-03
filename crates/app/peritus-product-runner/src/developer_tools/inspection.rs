//! Bounded read-only workspace inspection operations.

use std::{
    collections::VecDeque,
    fmt::Write as _,
    fs,
    io::{BufRead as _, BufReader},
    path::Path,
};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::{
    access_policy::WorkspaceAccessPolicy,
    effect::limit,
    path::{checked, ignored, tool},
    resources::CommandResources,
    wire::{bounded_usize, collection, object, required_string, string},
};
use crate::file_metadata;

pub(super) const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;

pub(super) fn list(
    root: &Path,
    arguments: &Value,
    resources: CommandResources,
    access_policy: &WorkspaceAccessPolicy,
) -> Result<Value, DeveloperLoopError> {
    let relative = string(arguments, "path").unwrap_or("");
    let depth = bounded_usize(arguments, "depth", 3, 1, 12);
    let start = if relative.is_empty() { root.to_owned() } else { checked(root, relative, false)? };
    let mut queue = VecDeque::from([(start, 0_usize)]);
    let mut entries = Vec::new();
    while let Some((directory, level)) = queue.pop_front() {
        let children = match fs::read_dir(&directory) {
            Ok(children) => children,
            Err(error) if level == 0 => return Err(tool(error.to_string())),
            Err(_) => continue,
        };
        let mut children = children.filter_map(Result::ok).collect::<Vec<_>>();
        children.sort_by_key(fs::DirEntry::file_name);
        for child in children {
            let path = child.path();
            let Some(relative) = path.strip_prefix(root).ok() else {
                continue;
            };
            if ignored(relative) || !access_policy.permits_search_result(relative) {
                continue;
            }
            let Ok(metadata) = fs::symlink_metadata(&path) else { continue };
            let kind = metadata.file_type();
            entries.push(object(vec![
                ("path", Value::String(relative.to_string_lossy().into_owned())),
                ("kind", Value::String(entry_kind(kind).to_owned())),
                ("bytes", Value::from(metadata.len())),
                ("permissions", Value::String(file_metadata::permissions(&metadata))),
            ]));
            if entries.len() >= 2_000 {
                return Ok(listing(root, entries, true, resources));
            }
            if kind.is_dir() && level + 1 < depth {
                queue.push_back((path, level + 1));
            }
        }
    }
    Ok(listing(root, entries, false, resources))
}

fn listing(
    root: &Path,
    entries: Vec<Value>,
    truncated: bool,
    resources: CommandResources,
) -> Value {
    object(vec![
        ("workspace_root", Value::String(root.to_string_lossy().into_owned())),
        ("path_kind", Value::String("workspace-relative".to_owned())),
        ("execution_resources", resources.observation()),
        ("entries", Value::Array(entries)),
        ("truncated", Value::Bool(truncated)),
    ])
}

pub(super) fn search(
    root: &Path,
    arguments: &Value,
    access_policy: &WorkspaceAccessPolicy,
) -> Result<Value, DeveloperLoopError> {
    let query = required_string(arguments, "query")?;
    if query.is_empty() {
        return Err(tool("search query is empty"));
    }
    let start = match string(arguments, "path") {
        Some(value) if !value.is_empty() => checked(root, value, false)?,
        _ => root.to_owned(),
    };
    let maximum = bounded_usize(arguments, "max_results", 200, 1, 1_000);
    let mut queue = VecDeque::from([start.clone()]);
    let mut matches = Vec::new();
    while let Some(path) = queue.pop_front() {
        if !access_policy.permits_search_result(path.strip_prefix(root).unwrap_or(&path)) {
            continue;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if path == start => return Err(tool(error.to_string())),
            Err(_) => continue,
        };
        if metadata.is_dir() {
            let children = match fs::read_dir(&path) {
                Ok(children) => children,
                Err(error) if path == start => return Err(tool(error.to_string())),
                Err(_) => continue,
            };
            let mut children = children.filter_map(Result::ok).collect::<Vec<_>>();
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
        let relative = path.strip_prefix(root).unwrap_or(&path);
        if !access_policy.permits_search_result(relative) {
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
    let path = checked(root, required_string(arguments, "path")?, true)?;
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            tool("not_found: this file does not exist. Use the observed workspace listing; do not repeat this read unless the file has since been created. For an authorized greenfield task, create the planned file with workspace_write rather than assuming a manifest already exists.")
        } else { tool(error.to_string()) }
    })?;
    if !metadata.is_file() {
        return Err(tool("path is not a regular text file"));
    }
    let start = bounded_usize(arguments, "start_line", 1, 1, usize::MAX);
    let default_end = start.saturating_add(499);
    let end = bounded_usize(arguments, "end_line", default_end, start, usize::MAX);
    let explicit_range =
        arguments.get("start_line").is_some() || arguments.get("end_line").is_some();
    if metadata.len() > MAX_FILE_BYTES as u64 && !explicit_range {
        return Err(tool(
            "file exceeds the inline byte bound; specify start_line and end_line to read a bounded range",
        ));
    }
    let lines = read_line_range(&path, start, end)?;
    Ok(object(vec![
        ("content", Value::String(limit(&lines))),
        ("start_line", Value::from(start)),
        ("end_line", Value::from(end)),
        ("bytes", Value::from(metadata.len())),
        ("permissions", Value::String(file_metadata::permissions(&metadata))),
    ]))
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

pub(super) fn read_line_range(
    path: &Path,
    start: usize,
    end: usize,
) -> Result<String, DeveloperLoopError> {
    let file = fs::File::open(path).map_err(|error| tool(error.to_string()))?;
    let mut reader = BufReader::new(file);
    let mut current = 1_usize;
    let mut selected = Vec::new();
    let mut output = String::new();
    loop {
        let available = reader.fill_buf().map_err(|error| tool(error.to_string()))?;
        if available.is_empty() || current > end {
            break;
        }
        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        let segment = &available[..consumed];
        if current >= start {
            if selected.len().saturating_add(segment.len()) > MAX_FILE_BYTES {
                return Err(tool("selected line range exceeds the inline byte bound"));
            }
            selected.extend_from_slice(segment);
        }
        let complete = segment.last() == Some(&b'\n');
        reader.consume(consumed);
        if complete {
            if current >= start {
                append_line(&mut output, current, &selected)?;
                selected.clear();
            }
            current = current.saturating_add(1);
        }
    }
    if current >= start && current <= end && !selected.is_empty() {
        append_line(&mut output, current, &selected)?;
    }
    Ok(output)
}

fn append_line(output: &mut String, number: usize, bytes: &[u8]) -> Result<(), DeveloperLoopError> {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    let line = std::str::from_utf8(bytes).map_err(|_| tool("selected line range is not UTF-8"))?;
    if !output.is_empty() {
        output.push('\n');
    }
    write!(output, "{number}: {line}").map_err(|error| tool(error.to_string()))
}
