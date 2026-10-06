//! Shared handle-relative discovery/search traversal and explicit native exclusions.

use super::{
    DiscoverExclusion, FsReadService, FsToolError, FsToolOperation, WalkObservation, WorkspacePath,
    bound_error, inspection_error, project_metadata,
};
use peritus_workspace::WorkspaceEntryKind;
use std::collections::VecDeque;

impl FsReadService<'_> {
    pub(super) fn walk(
        &self,
        root: Option<&WorkspacePath>,
        maximum_depth: u16,
        maximum_entries: u32,
        operation: FsToolOperation,
    ) -> Result<WalkObservation, FsToolError> {
        let mut pending = VecDeque::from([(root.cloned(), 0_u16)]);
        let mut observed = WalkObservation { entries: Vec::new(), exclusions: Vec::new() };
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
                if observed.entries.len() + observed.exclusions.len() >= maximum_entries as usize {
                    return Err(bound_error(operation, "workspace traversal entry bound exceeded"));
                }
                let depth = parent_depth.saturating_add(1);
                let metadata = match child.observation() {
                    Ok(metadata) => metadata,
                    Err(reason) => {
                        observed.exclusions.push(DiscoverExclusion {
                            directory: directory.clone(),
                            name: child.name().clone(),
                            reason,
                            depth,
                        });
                        continue;
                    }
                };
                let metadata = project_metadata(metadata);
                if metadata.kind == WorkspaceEntryKind::Directory && depth < maximum_depth {
                    pending.push_back((Some(metadata.path.clone()), depth));
                }
                observed.entries.push((metadata, depth));
            }
        }
        Ok(observed)
    }
}
