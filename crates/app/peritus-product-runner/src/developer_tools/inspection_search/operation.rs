//! Page orchestration for streaming literal workspace search.

use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use crate::developer_tools::{
    access_policy::WorkspaceAccessPolicy,
    inspection::{encoded_len, ignored_from, omission, page_bytes},
    inspection_cancellation::InspectionCancellation,
    inspection_cursor::CursorScope,
    path::{checked, tool},
    wire::{bounded_usize, object, required_string, string},
};

use super::scanner::{kmp_prefix, scan_file, utf8_prefix_len, validate_text_file};

const MATCH_PREVIEW_BYTES: usize = 16 * 1024;

pub(in crate::developer_tools) fn search(
    root: &Path,
    arguments: &Value,
    access_policy: &WorkspaceAccessPolicy,
    cancellation: &InspectionCancellation,
) -> Result<Value, DeveloperLoopError> {
    cancellation.check()?;
    let query = required_string(arguments, "query")?;
    if query.is_empty() {
        return Err(tool("search query is empty"));
    }
    let requested_path = string(arguments, "path").unwrap_or("");
    let start = if requested_path.is_empty() || requested_path == "." {
        root.to_owned()
    } else {
        checked(root, requested_path, false)?
    };
    let scope =
        CursorScope::new("workspace-search", root, &start).add_text(requested_path).add_text(query);
    let cursor = scope.read(arguments)?;
    let state = SearchState {
        root,
        start: start.clone(),
        start_relative: start.strip_prefix(root).unwrap_or_else(|_| Path::new("")).to_path_buf(),
        query: query.to_owned(),
        prefix: kmp_prefix(query.as_bytes()),
        access_policy,
        cancellation,
        scope,
        cursor,
        maximum: bounded_usize(arguments, "max_results", 200, 1, usize::MAX),
        max_bytes: page_bytes(arguments),
        queue: VecDeque::from([start]),
        ordinal: 0,
        returned: 0,
        matches: Vec::new(),
        omissions: Vec::new(),
        next_cursor: None,
        truncated: false,
    };
    state.walk()
}

struct SearchState<'a> {
    root: &'a Path,
    start: PathBuf,
    start_relative: PathBuf,
    query: String,
    prefix: Vec<usize>,
    access_policy: &'a WorkspaceAccessPolicy,
    cancellation: &'a InspectionCancellation,
    scope: CursorScope,
    cursor: usize,
    maximum: usize,
    max_bytes: usize,
    queue: VecDeque<PathBuf>,
    ordinal: usize,
    returned: usize,
    matches: Vec<Value>,
    omissions: Vec<Value>,
    next_cursor: Option<usize>,
    truncated: bool,
}

impl SearchState<'_> {
    fn walk(mut self) -> Result<Value, DeveloperLoopError> {
        while let Some(path) = self.queue.pop_front() {
            self.cancellation.check()?;
            let relative = path.strip_prefix(self.root).unwrap_or(&path);
            if !self.access_policy.permits_search_result(relative) {
                continue;
            }
            if !self.visit_path(&path, relative)? {
                break;
            }
        }
        if self.next_cursor.is_none() && self.ordinal < self.cursor {
            return Err(tool("workspace search cursor is beyond the current traversal"));
        }
        Ok(search_result(
            &self.matches,
            &self.omissions,
            self.truncated,
            self.next_cursor.map(|value| self.scope.value(value)),
        ))
    }

    fn visit_path(&mut self, path: &Path, relative: &Path) -> Result<bool, DeveloperLoopError> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if path == self.start.as_path() => {
                return Err(tool(error.to_string()));
            }
            Err(error) => return self.add_omission(omission(relative, error)),
        };
        if metadata.is_dir() {
            return self.visit_directory(path, relative);
        }
        if metadata.is_file() { self.visit_file(path, relative) } else { Ok(true) }
    }

    fn visit_directory(
        &mut self,
        path: &Path,
        relative: &Path,
    ) -> Result<bool, DeveloperLoopError> {
        let children = match fs::read_dir(path) {
            Ok(children) => children,
            Err(error) if path == self.start.as_path() => {
                return Err(tool(error.to_string()));
            }
            Err(error) => return self.add_omission(omission(relative, error)),
        };
        let mut readable = Vec::new();
        let mut iterator_errors = Vec::new();
        for child in children {
            self.cancellation.check()?;
            match child {
                Ok(child) => readable.push(child),
                Err(error) => iterator_errors.push(error),
            }
        }
        readable.sort_by_key(fs::DirEntry::file_name);
        for child in readable {
            self.cancellation.check()?;
            let child_path = child.path();
            if child_path
                .strip_prefix(self.root)
                .is_ok_and(|relative| ignored_from(&self.start_relative, relative))
            {
                continue;
            }
            self.queue.push_back(child_path);
        }
        for error in iterator_errors {
            self.cancellation.check()?;
            if !self.add_omission(omission(relative, error))? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn visit_file(&mut self, path: &Path, relative: &Path) -> Result<bool, DeveloperLoopError> {
        if let Err(error) = validate_text_file(path, self.cancellation) {
            return self.add_omission(object(vec![
                ("path", Value::String(relative.to_string_lossy().into_owned())),
                ("error", Value::String(error.to_string())),
            ]));
        }
        let query = self.query.clone();
        let query_bytes = query.as_bytes().to_vec();
        let prefix = self.prefix.clone();
        let cancellation = self.cancellation;
        let scope = self.scope;
        let max_bytes = self.max_bytes;
        let mut stop = false;
        let scan =
            scan_file(path, &query_bytes, &prefix, cancellation, |line, offset, line_bytes| {
                if self.ordinal < self.cursor {
                    self.ordinal = advance(self.ordinal)?;
                    return Ok(true);
                }
                if self.returned >= self.maximum {
                    self.next_cursor = Some(self.ordinal);
                    self.truncated = true;
                    stop = true;
                    return Ok(false);
                }
                let preview_len = utf8_prefix_len(&query, MATCH_PREVIEW_BYTES);
                let preview = &query[..preview_len];
                let result = object(vec![
                    ("path", Value::String(relative.to_string_lossy().into_owned())),
                    ("line", Value::from(line)),
                    ("text", Value::String(preview.to_owned())),
                    ("line_byte_offset", Value::from(offset)),
                    ("line_bytes", Value::from(line_bytes)),
                    (
                        "next_line_byte_offset",
                        if preview_len < query.len() {
                            Value::from(offset.saturating_add(preview_len as u64))
                        } else {
                            Value::Null
                        },
                    ),
                ]);
                let mut candidate = self.matches.clone();
                candidate.push(result.clone());
                if encoded_len(&search_result(
                    &candidate,
                    &self.omissions,
                    true,
                    Some(scope.value(usize::MAX)),
                ))? > max_bytes
                {
                    if self.matches.is_empty() && self.omissions.is_empty() {
                        return Err(tool("max_bytes is too small for the next workspace match"));
                    }
                    self.next_cursor = Some(self.ordinal);
                    self.truncated = true;
                    stop = true;
                    return Ok(false);
                }
                self.matches.push(result);
                self.returned = self.returned.saturating_add(1);
                self.ordinal = advance(self.ordinal)?;
                Ok(true)
            });
        match scan {
            Ok(false) if stop => Ok(false),
            Ok(_) => Ok(true),
            Err(error) => self.add_omission(omission(relative, error)),
        }
    }

    fn add_omission(&mut self, omission: Value) -> Result<bool, DeveloperLoopError> {
        if self.ordinal < self.cursor {
            self.ordinal = advance(self.ordinal)?;
            return Ok(true);
        }
        let mut candidate = self.omissions.clone();
        candidate.push(omission.clone());
        if encoded_len(&search_result(
            &self.matches,
            &candidate,
            true,
            Some(self.scope.value(usize::MAX)),
        ))? > self.max_bytes
        {
            if self.matches.is_empty() && self.omissions.is_empty() {
                return Err(tool("max_bytes is too small for the next workspace omission"));
            }
            self.next_cursor = Some(self.ordinal);
            self.truncated = true;
            return Ok(false);
        }
        self.omissions.push(omission);
        self.ordinal = advance(self.ordinal)?;
        Ok(true)
    }
}

fn search_result(
    matches: &[Value],
    omissions: &[Value],
    truncated: bool,
    next_cursor: Option<Value>,
) -> Value {
    object(vec![
        ("matches", Value::Array(matches.to_vec())),
        ("omissions", Value::Array(omissions.to_vec())),
        ("truncated", Value::Bool(truncated)),
        ("next_cursor", next_cursor.unwrap_or(Value::Null)),
    ])
}

fn advance(ordinal: usize) -> Result<usize, DeveloperLoopError> {
    ordinal.checked_add(1).ok_or_else(|| tool("workspace search cursor overflow"))
}
