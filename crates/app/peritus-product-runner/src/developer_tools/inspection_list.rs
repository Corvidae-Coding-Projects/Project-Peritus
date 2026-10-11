//! Bounded workspace directory listing with scope-bound continuations.

use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::{
    access_policy::WorkspaceAccessPolicy,
    inspection::{encoded_len, entry_kind, ignored_from, listing, omission, page_bytes},
    inspection_cancellation::InspectionCancellation,
    inspection_cursor::CursorScope,
    path::{checked, tool},
    resources::CommandResources,
    wire::{bounded_usize, object, string},
};

pub(super) fn list(
    root: &Path,
    arguments: &Value,
    resources: CommandResources,
    access_policy: &WorkspaceAccessPolicy,
    cancellation: &InspectionCancellation,
) -> Result<Value, DeveloperLoopError> {
    cancellation.check()?;
    let relative = string(arguments, "path").unwrap_or("");
    let depth = bounded_usize(arguments, "depth", 3, 1, usize::MAX);
    let max_bytes = page_bytes(arguments);
    let start = if relative.is_empty() || relative == "." {
        root.to_owned()
    } else {
        checked(root, relative, false)?
    };
    let scope =
        CursorScope::new("workspace-list", root, &start).add_text(relative).add_usize(depth);
    let cursor = scope.read(arguments)?;
    let page = ListPage {
        root,
        resources: &resources,
        access_policy,
        cancellation,
        scope,
        cursor,
        max_bytes,
    };
    let start_relative = start.strip_prefix(root).unwrap_or_else(|_| Path::new(""));
    let state = ListState::new(&start, start_relative);
    state.walk(&page, depth)
}

struct ListPage<'a> {
    root: &'a Path,
    resources: &'a CommandResources,
    access_policy: &'a WorkspaceAccessPolicy,
    cancellation: &'a InspectionCancellation,
    scope: CursorScope,
    cursor: usize,
    max_bytes: usize,
}

struct ListState {
    start_relative: PathBuf,
    queue: VecDeque<(PathBuf, usize)>,
    ordinal: usize,
    entries: Vec<Value>,
    omissions: Vec<Value>,
    next_cursor: Option<usize>,
    truncated: bool,
    depth_limited: bool,
}

impl ListState {
    fn new(start: &Path, start_relative: &Path) -> Self {
        Self {
            start_relative: start_relative.to_path_buf(),
            queue: VecDeque::from([(start.to_owned(), 0)]),
            ordinal: 0,
            entries: Vec::new(),
            omissions: Vec::new(),
            next_cursor: None,
            truncated: false,
            depth_limited: false,
        }
    }

    fn walk(mut self, page: &ListPage<'_>, depth: usize) -> Result<Value, DeveloperLoopError> {
        while let Some((directory, level)) = self.queue.pop_front() {
            if !self.visit_directory(page, &directory, level, depth)? {
                break;
            }
        }
        if self.next_cursor.is_none() && self.depth_limited {
            self.truncated = true;
        }
        if self.next_cursor.is_none() && self.ordinal < page.cursor {
            return Err(tool("workspace list cursor is beyond the current traversal"));
        }
        Ok(listing(
            page.root,
            self.entries,
            &self.omissions,
            self.truncated,
            self.next_cursor.map(|value| page.scope.value(value)),
            self.depth_limited,
            page.resources,
        ))
    }

    fn visit_directory(
        &mut self,
        page: &ListPage<'_>,
        directory: &Path,
        level: usize,
        depth: usize,
    ) -> Result<bool, DeveloperLoopError> {
        page.cancellation.check()?;
        let children = match fs::read_dir(directory) {
            Ok(children) => children,
            Err(error) if level == 0 => return Err(tool(error.to_string())),
            Err(error) => {
                return self.add_omission(
                    page,
                    omission(directory.strip_prefix(page.root).unwrap_or(directory), error),
                );
            }
        };
        let mut readable = Vec::new();
        let mut iterator_errors = Vec::new();
        for child in children {
            page.cancellation.check()?;
            match child {
                Ok(child) => readable.push(child),
                Err(error) => iterator_errors.push(error),
            }
        }
        readable.sort_by_key(fs::DirEntry::file_name);
        for child in readable {
            if !self.visit_child(page, &child, level, depth)? {
                return Ok(false);
            }
        }
        for error in iterator_errors {
            page.cancellation.check()?;
            if !self.add_omission(
                page,
                omission(directory.strip_prefix(page.root).unwrap_or(directory), error),
            )? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn visit_child(
        &mut self,
        page: &ListPage<'_>,
        child: &fs::DirEntry,
        level: usize,
        depth: usize,
    ) -> Result<bool, DeveloperLoopError> {
        page.cancellation.check()?;
        let path = child.path();
        let Ok(relative) = path.strip_prefix(page.root) else { return Ok(true) };
        if ignored_from(&self.start_relative, relative)
            || !page.access_policy.permits_search_result(relative)
        {
            return Ok(true);
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                return self.add_omission(page, omission(relative, error));
            }
        };
        let kind = metadata.file_type();
        let entry = object(vec![
            ("path", Value::String(relative.to_string_lossy().into_owned())),
            ("kind", Value::String(entry_kind(kind).to_owned())),
            ("bytes", Value::from(metadata.len())),
            ("permissions", Value::String(crate::file_metadata::permissions(&metadata))),
        ]);
        if self.ordinal >= page.cursor {
            let mut candidate = self.entries.clone();
            candidate.push(entry.clone());
            if encoded_len(&listing(
                page.root,
                candidate,
                &self.omissions,
                true,
                Some(page.scope.value(usize::MAX)),
                self.depth_limited,
                page.resources,
            ))? > page.max_bytes
            {
                if self.entries.is_empty() && self.omissions.is_empty() {
                    return Err(tool("max_bytes is too small for the next workspace entry"));
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
        page: &ListPage<'_>,
        omission: Value,
    ) -> Result<bool, DeveloperLoopError> {
        if self.ordinal < page.cursor {
            self.ordinal = self.ordinal.saturating_add(1);
            return Ok(true);
        }
        let mut candidate = self.omissions.clone();
        candidate.push(omission.clone());
        if encoded_len(&listing(
            page.root,
            self.entries.clone(),
            &candidate,
            true,
            Some(page.scope.value(usize::MAX)),
            false,
            page.resources,
        ))? > page.max_bytes
        {
            if self.entries.is_empty() && self.omissions.is_empty() {
                return Err(tool("max_bytes is too small for the next workspace omission"));
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
