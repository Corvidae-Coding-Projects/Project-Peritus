//! Concrete bounded filesystem and structured-command developer tools.

use std::{collections::VecDeque, fs, path::PathBuf, process::Command};

use peritus_agent::{DeveloperLoopError, DeveloperToolExecutor, DeveloperToolObservation};
use peritus_model_protocol::{CanonicalJson, CompletedToolCall, JsonBounds, ProtocolLimits};
use serde_json::{Map, Value};

use super::{
    effect::{atomic_write, atomic_write_if_changed, limit, reject_destructive_command},
    grounding::GroundingEvidence,
    ownership::WorkspaceOwnership,
    path::{checked, ignored, tool},
    removal,
};

const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkspaceToolMode {
    ReadWrite,
    ReadOnly,
}

/// Concrete tool executor scoped to one managed workspace.
pub struct WorkspaceDeveloperTools {
    root: PathBuf,
    grounding: GroundingEvidence,
    ownership: WorkspaceOwnership,
    mode: WorkspaceToolMode,
}

impl WorkspaceDeveloperTools {
    /// Creates an executor that rejects every mutating or process tool even if a provider emits an
    /// undeclared call.
    #[must_use]
    pub fn read_only(root: PathBuf) -> Self {
        let ownership = WorkspaceOwnership::capture(&root);
        Self {
            root,
            grounding: GroundingEvidence::default(),
            ownership,
            mode: WorkspaceToolMode::ReadOnly,
        }
    }

    pub(crate) fn with_ownership(root: PathBuf, ownership: WorkspaceOwnership) -> Self {
        Self {
            root,
            grounding: GroundingEvidence::default(),
            ownership,
            mode: WorkspaceToolMode::ReadWrite,
        }
    }

    pub const fn grounding(&self) -> &GroundingEvidence {
        &self.grounding
    }

    pub(crate) const fn ownership(&self) -> &WorkspaceOwnership {
        &self.ownership
    }
}

impl DeveloperToolExecutor for WorkspaceDeveloperTools {
    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        let arguments: Value = serde_json::from_slice(call.arguments().canonical_bytes())
            .map_err(|error| tool(error.to_string()))?;
        let result = match call.name().as_str() {
            "workspace_list" => self.list(&arguments),
            "workspace_search" => self.search(&arguments),
            "workspace_read" => self.read(&arguments),
            "workspace_write" | "workspace_patch" | "workspace_remove" | "run_command"
                if self.mode == WorkspaceToolMode::ReadOnly =>
            {
                Err(tool("this role has read-only workspace access"))
            }
            "workspace_write" => self.write(&arguments),
            "workspace_patch" => self.patch(&arguments),
            "workspace_remove" => self.remove(&arguments),
            "run_command" => self.run(&arguments),
            _ => return Err(tool("model requested an undeclared developer tool")),
        };
        match result {
            Ok(value) => {
                self.record_success(call.name().as_str(), &arguments, &value);
                let is_error = value.get("success").and_then(Value::as_bool) == Some(false);
                observation(&value, is_error)
            }
            Err(error) => {
                let value = object(vec![("error", Value::String(error.to_string()))]);
                observation(&value, true)
            }
        }
    }
}

impl WorkspaceDeveloperTools {
    fn record_success(&mut self, name: &str, arguments: &Value, result: &Value) {
        match name {
            "workspace_list" => self.grounding.record_list(
                string(arguments, "path").unwrap_or(""),
                result.get("entries").and_then(Value::as_array).map_or(0, Vec::len),
            ),
            "workspace_search" => self.grounding.record_search(),
            "workspace_read" => {
                if let Some(path) = string(arguments, "path") {
                    self.grounding.record_read(path);
                }
            }
            "workspace_write" | "workspace_patch" | "workspace_remove" => {
                if let Some(path) = string(arguments, "path") {
                    self.grounding.record_mutation(path);
                }
            }
            _ => {}
        }
        if name == "workspace_list" {
            for path in result
                .get("entries")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.get("path").and_then(Value::as_str))
            {
                self.grounding.record_listed_path(path);
            }
        }
    }

    fn list(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let relative = string(arguments, "path").unwrap_or("");
        let depth = bounded_usize(arguments, "depth", 3, 1, 12);
        let start = if relative.is_empty() {
            self.root.clone()
        } else {
            checked(&self.root, relative, false)?
        };
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
                let Some(relative) = path.strip_prefix(&self.root).ok() else {
                    continue;
                };
                if ignored(relative) {
                    continue;
                }
                let kind = child.file_type().map_err(|error| tool(error.to_string()))?;
                entries.push(object(vec![
                    ("path", Value::String(relative.to_string_lossy().into_owned())),
                    (
                        "kind",
                        Value::String(if kind.is_dir() { "directory" } else { "file" }.to_owned()),
                    ),
                ]));
                if entries.len() >= 2_000 {
                    return Ok(collection("entries", entries, true));
                }
                if kind.is_dir() && level + 1 < depth {
                    queue.push_back((path, level + 1));
                }
            }
        }
        Ok(collection("entries", entries, false))
    }

    fn search(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let query = required_string(arguments, "query")?;
        if query.is_empty() {
            return Err(tool("search query is empty"));
        }
        let start = match string(arguments, "path") {
            Some(value) if !value.is_empty() => checked(&self.root, value, false)?,
            _ => self.root.clone(),
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
                    if child.path().strip_prefix(&self.root).is_ok_and(ignored) {
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
                                path.strip_prefix(&self.root)
                                    .unwrap_or(&path)
                                    .to_string_lossy()
                                    .into_owned(),
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

    fn read(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let path = checked(&self.root, required_string(arguments, "path")?, false)?;
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
        ]))
    }

    fn write(&mut self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let relative = required_string(arguments, "path")?;
        let content = required_string(arguments, "content")?;
        if content.len() > MAX_FILE_BYTES {
            return Err(tool("write exceeds the per-file byte bound"));
        }
        let path = checked(&self.root, relative, true)?;
        let existed_before = path.exists();
        self.grounding.ensure_mutation_allowed(relative, existed_before).map_err(tool)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| tool(error.to_string()))?;
        }
        let changed = atomic_write_if_changed(&path, content.as_bytes())?;
        self.ownership.record_direct_creation(&path, existed_before);
        Ok(object(vec![
            ("path", Value::String(relative.to_owned())),
            ("bytes", Value::from(content.len())),
            ("changed", Value::Bool(changed)),
        ]))
    }

    fn patch(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let relative = required_string(arguments, "path")?;
        let old = required_string(arguments, "old")?;
        let new = required_string(arguments, "new")?;
        let replace_all = arguments.get("replace_all").and_then(Value::as_bool).unwrap_or(false);
        if old.is_empty() {
            return Err(tool("patch old text is empty"));
        }
        let path = checked(&self.root, relative, false)?;
        self.grounding.ensure_mutation_allowed(relative, true).map_err(tool)?;
        let content = fs::read_to_string(&path).map_err(|error| tool(error.to_string()))?;
        let occurrences = content.matches(old).count();
        if occurrences == 0 || (!replace_all && occurrences != 1) {
            return Err(tool(format!("patch expected one match but found {occurrences}")));
        }
        let replaced =
            if replace_all { content.replace(old, new) } else { content.replacen(old, new, 1) };
        if replaced.len() > MAX_FILE_BYTES {
            return Err(tool("patched file exceeds the per-file byte bound"));
        }
        atomic_write(&path, replaced.as_bytes())?;
        Ok(object(vec![
            ("path", Value::String(relative.to_owned())),
            ("replacements", Value::from(if replace_all { occurrences } else { 1 })),
        ]))
    }

    fn remove(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        removal::remove(&self.root, &self.grounding, &self.ownership, arguments)
    }

    fn run(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let program = required_string(arguments, "program")?;
        if program.is_empty() || program.contains(['\0', '\n', '\r']) {
            return Err(tool("command program is invalid"));
        }
        let args = arguments
            .get("args")
            .and_then(Value::as_array)
            .ok_or_else(|| tool("command args must be an array"))?
            .iter()
            .map(|value| {
                value.as_str().map(str::to_owned).ok_or_else(|| tool("command arg is not text"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        reject_destructive_command(program, &args)?;
        let current_dir = match string(arguments, "cwd") {
            Some(value) if !value.is_empty() => checked(&self.root, value, false)?,
            _ => self.root.clone(),
        };
        let mut command = Command::new(program);
        command.args(&args).current_dir(current_dir);
        if program == "cargo" {
            command.env("CARGO_BUILD_JOBS", "2");
        }
        let output = command.output().map_err(|error| tool(error.to_string()))?;
        Ok(object(vec![
            ("success", Value::Bool(output.status.success())),
            ("exit_code", output.status.code().map_or(Value::Null, Value::from)),
            ("stdout", Value::String(limit(&String::from_utf8_lossy(&output.stdout)))),
            ("stderr", Value::String(limit(&String::from_utf8_lossy(&output.stderr)))),
        ]))
    }
}

fn object(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries.into_iter().map(|(key, value)| (key.to_owned(), value)).collect::<Map<_, _>>(),
    )
}

fn collection(name: &str, values: Vec<Value>, truncated: bool) -> Value {
    object(vec![(name, Value::Array(values)), ("truncated", Value::Bool(truncated))])
}

fn observation(
    value: &Value,
    is_error: bool,
) -> Result<DeveloperToolObservation, DeveloperLoopError> {
    let encoded = serde_json::to_string(&value).map_err(|error| tool(error.to_string()))?;
    let output = CanonicalJson::parse(&encoded, JsonBounds::value(ProtocolLimits::PRODUCTION))?;
    Ok(DeveloperToolObservation { output, is_error })
}

fn required_string<'a>(value: &'a Value, name: &str) -> Result<&'a str, DeveloperLoopError> {
    string(value, name).ok_or_else(|| tool(format!("{name} must be text")))
}

fn string<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value.get(name).and_then(Value::as_str)
}

fn bounded_usize(
    value: &Value,
    name: &str,
    default: usize,
    minimum: usize,
    maximum: usize,
) -> usize {
    value
        .get(name)
        .and_then(Value::as_u64)
        .and_then(|number| usize::try_from(number).ok())
        .unwrap_or(default)
        .clamp(minimum, maximum)
}

#[cfg(test)]
mod tests;
