//! Incremental deterministic traversal over checked C1 directory pages.

use std::collections::VecDeque;

use peritus_patch::WorkspacePath;
use peritus_workspace::{DirectoryCursor, DirectoryEntry, WorkspaceEntryKind};

use super::{FsReadService, MetadataObservation, bound_error, inspection_error, project_metadata};
use crate::{FsToolError, FsToolOperation, OmissionReason, ScopeOmission};

impl FsReadService<'_> {
    /// Visits a depth-first canonical traversal while retaining only ancestor directory pages.
    pub(super) fn walk_visit(
        &self,
        root: Option<&WorkspacePath>,
        maximum_depth: u16,
        operation: FsToolOperation,
        cancelled: &dyn Fn() -> bool,
        mut visit: impl FnMut(MetadataObservation, u16, Option<ScopeOmission>) -> bool,
    ) -> Result<bool, FsToolError> {
        struct Frame {
            directory: Option<WorkspacePath>,
            depth: u16,
            children: VecDeque<DirectoryEntry>,
            cursor: Option<DirectoryCursor>,
        }

        let first = self
            .workspace
            .list_directory_page_cancellable(root, None, cancelled)
            .map_err(|error| inspection_error(operation, &error))?;
        let Some(first) = first else { return Ok(false) };
        let (children, cursor) = first.into_parts();
        let mut stack =
            vec![Frame { directory: root.cloned(), depth: 0, children: children.into(), cursor }];
        while !stack.is_empty() {
            if cancelled() {
                return Ok(false);
            }
            if stack.last().is_some_and(|frame| frame.children.is_empty()) {
                let Some(cursor) = stack.last().and_then(|frame| frame.cursor.clone()) else {
                    stack.pop();
                    continue;
                };
                let directory = stack.last().and_then(|frame| frame.directory.clone());
                let page = self
                    .workspace
                    .list_directory_page_cancellable(directory.as_ref(), Some(&cursor), cancelled)
                    .map_err(|error| inspection_error(operation, &error))?;
                let Some(page) = page else { return Ok(false) };
                let (children, cursor) = page.into_parts();
                let Some(frame) = stack.last_mut() else {
                    return Err(bound_error(operation, "directory traversal stack was lost"));
                };
                frame.children = children.into();
                frame.cursor = cursor;
                continue;
            }
            let Some(child) = stack.last_mut().and_then(|frame| frame.children.pop_front()) else {
                stack.pop();
                continue;
            };
            let depth =
                stack.last().map_or(0, |frame| frame.depth).checked_add(1).ok_or_else(|| {
                    bound_error(operation, "workspace traversal depth is not representable")
                })?;
            let metadata = project_metadata(child.metadata());
            let mut omission = None;
            if metadata.kind == WorkspaceEntryKind::Directory {
                if depth < maximum_depth {
                    let children = self
                        .workspace
                        .list_directory_page_cancellable(Some(&metadata.path), None, cancelled)
                        .map_err(|error| inspection_error(operation, &error))?;
                    let Some(children) = children else { return Ok(false) };
                    let (children, cursor) = children.into_parts();
                    stack.push(Frame {
                        directory: Some(metadata.path.clone()),
                        depth,
                        children: children.into(),
                        cursor,
                    });
                } else {
                    omission = Some(ScopeOmission {
                        path: metadata.path.clone(),
                        reason: OmissionReason::DepthLimit,
                    });
                }
            } else if metadata.kind == WorkspaceEntryKind::Other {
                omission = Some(ScopeOmission {
                    path: metadata.path.clone(),
                    reason: OmissionReason::UnsafeEntry,
                });
            }
            if !visit(metadata, depth, omission) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
