//! Shared handle-relative discovery/search traversal and explicit native exclusions.

use super::{
    DiscoverExclusion, FsReadService, FsToolError, FsToolOperation, WalkObservation, WalkRecord,
    WorkspacePath, inspection_error, project_metadata,
};
use crate::exclusion::TraversalOmission;
use peritus_workspace::WorkspaceEntryKind;
use std::collections::VecDeque;

impl FsReadService<'_> {
    pub(super) fn walk(
        &self,
        root: Option<&WorkspacePath>,
        maximum_depth: u16,
        operation: FsToolOperation,
    ) -> Result<WalkObservation, FsToolError> {
        let mut pending = VecDeque::from([(root.cloned(), 0_u16)]);
        let mut observed = WalkObservation { records: Vec::new() };
        while let Some((directory, parent_depth)) = pending.pop_front() {
            let children = self
                .workspace
                .inspect_directory(directory.as_ref())
                .map_err(|error| inspection_error(operation, &error))?;
            let mut children = children.items().to_vec();
            children.sort_unstable_by(|left, right| match (left.metadata(), right.metadata()) {
                (Some(left), Some(right)) => left.path().cmp(right.path()),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => left.name().encoded_bytes().cmp(right.name().encoded_bytes()),
            });
            for child in children {
                let depth = parent_depth.saturating_add(1);
                let metadata = match child.observation() {
                    Ok(metadata) => metadata,
                    Err(reason) => {
                        observed.records.push(WalkRecord::Exclusion(DiscoverExclusion {
                            directory: directory.clone(),
                            name: child.name().clone(),
                            reason,
                            depth,
                        }));
                        continue;
                    }
                };
                let metadata = project_metadata(metadata);
                let descend = metadata.kind == WorkspaceEntryKind::Directory;
                let path = metadata.path.clone();
                observed.records.push(WalkRecord::Entry(metadata, depth));
                if descend {
                    if depth < maximum_depth {
                        pending.push_back((Some(path), depth));
                    } else {
                        observed.records.push(WalkRecord::TraversalOmission(TraversalOmission {
                            path,
                            depth,
                        }));
                    }
                }
            }
        }
        Ok(observed)
    }
}
