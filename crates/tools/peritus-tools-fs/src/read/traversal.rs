//! Incremental deterministic traversal over checked C1 directory pages.

use std::collections::VecDeque;

use peritus_patch::WorkspacePath;
use peritus_workspace::{
    DirectoryCursor, DirectoryDiagnostic, DirectoryDiagnosticKind, DirectoryEntry,
    WorkspaceEntryKind,
};

use super::{FsReadService, MetadataObservation, bound_error, inspection_error, project_metadata};
use crate::{FsToolError, FsToolOperation, OmissionReason, ScopeOmission};

pub(super) enum WalkEvent {
    Diagnostic(ScopeOmission),
    Entry { metadata: MetadataObservation, depth: u16, omission: Option<ScopeOmission> },
}

impl FsReadService<'_> {
    /// Visits a depth-first canonical traversal while retaining only ancestor directory pages.
    pub(super) fn walk_visit(
        &self,
        root: Option<&WorkspacePath>,
        maximum_depth: u16,
        operation: FsToolOperation,
        cancelled: &dyn Fn() -> bool,
        mut visit: impl FnMut(WalkEvent) -> bool,
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
        let (children, page_diagnostics, cursor) = first.into_parts();
        if !visit_diagnostics(root, &page_diagnostics, cancelled, &mut visit) {
            return Ok(false);
        }
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
                let (children, page_diagnostics, cursor) = page.into_parts();
                if !visit_diagnostics(directory.as_ref(), &page_diagnostics, cancelled, &mut visit) {
                    return Ok(false);
                }
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
                    let (children, page_diagnostics, cursor) = children.into_parts();
                    if !visit_diagnostics(Some(&metadata.path), &page_diagnostics, cancelled, &mut visit) {
                        return Ok(false);
                    }
                    stack.push(Frame {
                        directory: Some(metadata.path.clone()),
                        depth,
                        children: children.into(),
                        cursor,
                    });
                } else {
                    omission = Some(ScopeOmission::at_path(
                        metadata.path.clone(),
                        OmissionReason::DepthLimit,
                    ));
                }
            } else if metadata.kind == WorkspaceEntryKind::Other {
                omission = Some(ScopeOmission::at_path(
                    metadata.path.clone(),
                    OmissionReason::UnsafeEntry,
                ));
            }
            if !visit(WalkEvent::Entry { metadata, depth, omission }) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn visit_diagnostics(
    directory: Option<&WorkspacePath>,
    diagnostics: &[DirectoryDiagnostic],
    cancelled: &dyn Fn() -> bool,
    visit: &mut impl FnMut(WalkEvent) -> bool,
) -> bool {
    diagnostics.iter().all(|value| {
        !cancelled() && visit(WalkEvent::Diagnostic(project_diagnostic(directory, value)))
    })
}

fn project_diagnostic(
    directory: Option<&WorkspacePath>,
    diagnostic: &DirectoryDiagnostic,
) -> ScopeOmission {
    let mut native_path = directory.map_or_else(Vec::new, |path| path.as_str().as_bytes().to_vec());
    if !native_path.is_empty() {
        native_path.push(b'/');
    }
    native_path.extend_from_slice(diagnostic.name_bytes());
    let reason = match diagnostic.kind() {
        DirectoryDiagnosticKind::UnsupportedName => OmissionReason::UnsupportedName,
        DirectoryDiagnosticKind::UnsupportedType => OmissionReason::UnsupportedType,
    };
    ScopeOmission::native_path(native_path, reason)
}
