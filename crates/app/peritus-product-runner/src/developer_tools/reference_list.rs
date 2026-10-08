//! Bounded listing of paths explicitly authorized outside the workspace.

use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::{
    inspection::entry_kind,
    inspection_cancellation::InspectionCancellation,
    inspection_cursor::CursorScope,
    path::{ignored, tool},
    reference::{ExplicitReferences, reference_io_error},
    wire::{bounded_usize, object, required_string},
};
use crate::file_metadata;

const MAX_PAGE_BYTES: usize = 512 * 1024;

pub(super) fn list(
    references: &ExplicitReferences,
    arguments: &Value,
    cancellation: &InspectionCancellation,
) -> Result<Value, DeveloperLoopError> {
    cancellation.check()?;
    let requested = required_string(arguments, "path")?;
    let (root, start) = references.resolve(requested)?;
    let metadata = fs::symlink_metadata(&start).map_err(|error| reference_io_error(&error))?;
    if !metadata.is_dir() {
        return Err(tool("external reference path is not a directory"));
    }
    let depth = bounded_usize(arguments, "depth", 3, 1, usize::MAX);
    let max_bytes = bounded_usize(arguments, "max_bytes", MAX_PAGE_BYTES, 256, MAX_PAGE_BYTES);
    let scope = CursorScope::new("external-reference-list", &root, &start)
        .add_text(requested)
        .add_usize(depth);
    let cursor = scope.read(arguments)?;
    let state = ReferenceState::new(&root, &start);
    let page = ReferencePage { root, cancellation, scope, cursor, max_bytes };
    state.walk(&page, depth)
}

struct ReferencePage<'a> {
    root: PathBuf,
    cancellation: &'a InspectionCancellation,
    scope: CursorScope,
    cursor: usize,
    max_bytes: usize,
}

struct ReferenceState {
    start_relative: PathBuf,
    queue: VecDeque<(PathBuf, usize)>,
    ordinal: usize,
    entries: Vec<Value>,
    omissions: Vec<Value>,
    next_cursor: Option<usize>,
    truncated: bool,
    depth_limited: bool,
}

impl ReferenceState {
    fn new(root: &Path, start: &Path) -> Self {
        Self {
            start_relative: start
                .strip_prefix(root)
                .unwrap_or_else(|_| Path::new(""))
                .to_path_buf(),
            queue: VecDeque::from([(start.to_owned(), 0)]),
            ordinal: 0,
            entries: Vec::new(),
            omissions: Vec::new(),
            next_cursor: None,
            truncated: false,
            depth_limited: false,
        }
    }

    fn walk(mut self, page: &ReferencePage<'_>, depth: usize) -> Result<Value, DeveloperLoopError> {
        while let Some((directory, level)) = self.queue.pop_front() {
            if !self.visit_directory(page, &directory, level, depth)? {
                break;
            }
        }
        if self.next_cursor.is_none() && self.depth_limited {
            self.truncated = true;
        }
        if self.next_cursor.is_none() && self.ordinal < page.cursor {
            return Err(tool("external reference cursor is beyond the current traversal"));
        }
        Ok(reference_list(
            &page.root,
            self.entries,
            &self.omissions,
            self.truncated,
            self.next_cursor.map(|value| page.scope.value(value)),
            self.depth_limited,
        ))
    }

    fn visit_directory(
        &mut self,
        page: &ReferencePage<'_>,
        directory: &Path,
        level: usize,
        depth: usize,
    ) -> Result<bool, DeveloperLoopError> {
        page.cancellation.check()?;
        let read_dir = match fs::read_dir(directory) {
            Ok(children) => children,
            Err(error) if level == 0 => return Err(tool(error.to_string())),
            Err(error) => return self.add_omission(page, reference_omission(directory, &error)),
        };
        for child in read_dir {
            page.cancellation.check()?;
            match child {
                Ok(child) => {
                    if !self.visit_child(page, &child, level, depth)? {
                        return Ok(false);
                    }
                }
                Err(error) => {
                    if !self.add_omission(page, reference_omission(directory, &error))? {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }

    fn visit_child(
        &mut self,
        page: &ReferencePage<'_>,
        child: &fs::DirEntry,
        level: usize,
        depth: usize,
    ) -> Result<bool, DeveloperLoopError> {
        page.cancellation.check()?;
        let path = child.path();
        let relative = path.strip_prefix(&page.root).unwrap_or(&path);
        if ignored_from(&self.start_relative, relative) {
            return Ok(true);
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => return self.add_omission(page, reference_omission(&path, &error)),
        };
        let kind = metadata.file_type();
        if self.ordinal >= page.cursor {
            let entry = object(vec![
                ("path", Value::String(path.to_string_lossy().into_owned())),
                ("kind", Value::String(entry_kind(kind).to_owned())),
                ("bytes", Value::from(metadata.len())),
                ("permissions", Value::String(file_metadata::permissions(&metadata))),
            ]);
            let mut candidate = self.entries.clone();
            candidate.push(entry.clone());
            if reference_list_len(
                &page.root,
                &candidate,
                &self.omissions,
                true,
                Some(page.scope.value(usize::MAX)),
            )? > page.max_bytes
            {
                if self.entries.is_empty() && self.omissions.is_empty() {
                    return Err(tool(
                        "max_bytes is too small for the next external reference entry",
                    ));
                }
                self.next_cursor = Some(self.ordinal);
                self.truncated = true;
                return Ok(false);
            }
            self.entries.push(entry);
        }
        self.ordinal = self.ordinal.saturating_add(1);
        if kind.is_dir() {
            if level.saturating_add(1) < depth {
                self.queue.push_back((path, level.saturating_add(1)));
            } else {
                self.depth_limited = true;
            }
        }
        Ok(true)
    }

    fn add_omission(
        &mut self,
        page: &ReferencePage<'_>,
        omission: Value,
    ) -> Result<bool, DeveloperLoopError> {
        if self.ordinal < page.cursor {
            self.ordinal = self.ordinal.saturating_add(1);
            return Ok(true);
        }
        let mut candidate = self.omissions.clone();
        candidate.push(omission.clone());
        if reference_list_len(
            &page.root,
            &self.entries,
            &candidate,
            true,
            Some(page.scope.value(usize::MAX)),
        )? > page.max_bytes
        {
            if self.entries.is_empty() && self.omissions.is_empty() {
                return Err(tool(
                    "max_bytes is too small for the next external reference omission",
                ));
            }
            self.next_cursor = Some(self.ordinal);
            self.truncated = true;
            return Ok(false);
        }
        self.omissions.push(omission);
        self.ordinal = self.ordinal.saturating_add(1);
        Ok(true)
    }
}

fn reference_list_len(
    root: &Path,
    entries: &[Value],
    omissions: &[Value],
    truncated: bool,
    next_cursor: Option<Value>,
) -> Result<usize, DeveloperLoopError> {
    serde_json::to_vec(&reference_list(
        root,
        entries.to_vec(),
        omissions,
        truncated,
        next_cursor,
        false,
    ))
    .map(|bytes| bytes.len())
    .map_err(|error| tool(error.to_string()))
}

fn reference_list(
    root: &Path,
    entries: Vec<Value>,
    omissions: &[Value],
    truncated: bool,
    next_cursor: Option<Value>,
    depth_limited: bool,
) -> Value {
    object(vec![
        ("reference_root", Value::String(root.to_string_lossy().into_owned())),
        ("path_kind", Value::String("explicit-absolute-reference".to_owned())),
        ("entries", Value::Array(entries)),
        ("omissions", Value::Array(omissions.to_vec())),
        ("truncated", Value::Bool(truncated)),
        ("next_cursor", next_cursor.unwrap_or(Value::Null)),
        ("depth_limited", Value::Bool(depth_limited)),
    ])
}

fn reference_omission(path: &Path, error: &std::io::Error) -> Value {
    object(vec![
        ("path", Value::String(path.to_string_lossy().into_owned())),
        ("error", Value::String(error.to_string())),
    ])
}

fn ignored_from(start: &Path, candidate: &Path) -> bool {
    if start.as_os_str().is_empty() {
        ignored(candidate)
    } else {
        candidate.strip_prefix(start).map_or_else(|_| ignored(candidate), ignored)
    }
}
