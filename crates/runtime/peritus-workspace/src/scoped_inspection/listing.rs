//! Stable handle-relative listing with typed exclusions for individual native children.

use super::directory::same_directory;
use super::{
    FolderIdentity, FolderInspection, NativeEntryName, WorkspaceError, changed, invalid,
    reject_cap_reparse, snapshot_io,
};
use crate::{WorkspaceEntryKind, WorkspaceMetadata};
use cap_fs_ext::MetadataExt as _;
use cap_std::fs::{Dir, Metadata};
use peritus_patch::WorkspacePath;

mod membership;
mod record;
mod retained;
mod storage;
#[cfg(test)]
mod tests;
pub use record::{DirectoryCursor, ObservedDirectory};
pub use retained::{DirectoryPage, RetainedDirectory};

/// Why one ordinary child is excluded from path-based inspection or effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectoryExclusionReason {
    /// Exact native name cannot be represented by the host UTF-8 authority path type.
    UnrepresentableName,
    /// A symbolic link is observed but never traversed.
    SymbolicLink,
    /// Windows reparse point is observed but never traversed.
    ReparsePoint,
    /// A socket, FIFO, device, or another non-file/non-directory node.
    SpecialNode,
    /// Child metadata is unavailable; the native name is still observed exactly.
    MetadataUnavailable,
    /// Child identity or metadata changed during its observation.
    Changed,
}

impl DirectoryExclusionReason {
    /// Returns the stable machine-readable exclusion label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnrepresentableName => "unrepresentable_name",
            Self::SymbolicLink => "symbolic_link",
            Self::ReparsePoint => "reparse_point",
            Self::SpecialNode => "special_node",
            Self::MetadataUnavailable => "metadata_unavailable",
            Self::Changed => "changed",
        }
    }
}

/// One native child and its supported metadata or explicit exclusion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryItem {
    pub(super) name: NativeEntryName,
    pub(super) observation: Result<WorkspaceMetadata, DirectoryExclusionReason>,
}
impl DirectoryItem {
    /// Borrows the exact native child identity, independently of its display.
    #[must_use]
    pub const fn name(&self) -> &NativeEntryName {
        &self.name
    }
    /// Borrows supported metadata, if this child has an authority-safe path.
    #[must_use]
    pub fn metadata(&self) -> Option<&WorkspaceMetadata> {
        self.observation.as_ref().ok()
    }
    /// Returns the supported observation or typed exclusion in one exhaustive result.
    /// This evidence does not grant mutation or traversal authority.
    ///
    /// # Errors
    /// Returns the observed per-child exclusion, without failing usable sibling observations.
    pub fn observation(&self) -> Result<&WorkspaceMetadata, DirectoryExclusionReason> {
        self.observation.as_ref().map_err(|reason| *reason)
    }
    /// Returns the typed exclusion reason, when the child is unsupported.
    #[must_use]
    pub fn exclusion(&self) -> Option<DirectoryExclusionReason> {
        self.observation.as_ref().err().copied()
    }
}

/// Complete directory observation in exact native-name order, including exclusions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryListing {
    path: Option<WorkspacePath>,
    items: Vec<DirectoryItem>,
}
impl DirectoryListing {
    /// Returns the observed relative directory; `None` means the bound root.
    #[must_use]
    pub const fn path(&self) -> Option<&WorkspacePath> {
        self.path.as_ref()
    }
    /// Borrows all exposed native children, including typed exclusions.
    #[must_use]
    pub fn items(&self) -> &[DirectoryItem] {
        &self.items
    }
}

impl FolderInspection {
    /// Observes one directory without letting unsupported children discard usable siblings.
    /// Protected metadata is hidden. Each child is observed relative to the opened handle;
    /// this listing is evidence, not effect authority or an atomic snapshot of child contents.
    ///
    /// # Errors
    /// Rejects an unsafe directory, changed directory/root identity, or incomplete iteration.
    pub fn inspect_directory(
        &self,
        path: Option<&WorkspacePath>,
    ) -> Result<DirectoryListing, WorkspaceError> {
        let mut items = Vec::new();
        self.scan_directory(path, None, |item| {
            items.push(item);
            Ok(())
        })?;
        items.sort_unstable_by(|left, right| {
            left.name.encoded_bytes().cmp(right.name.encoded_bytes())
        });
        Ok(DirectoryListing { path: path.cloned(), items })
    }
    /// Observes exact supported child metadata without following any ancestor or target link.
    ///
    /// # Errors
    /// Rejects links, special nodes, changed identity, unsafe paths and original I/O failures.
    pub fn metadata(&self, path: &WorkspacePath) -> Result<WorkspaceMetadata, WorkspaceError> {
        let (parent, name) = path
            .as_str()
            .rsplit_once('/')
            .map_or((None, path.as_str()), |(parent, name)| (Some(parent), name));
        let parent_path = parent
            .map(WorkspacePath::new)
            .transpose()
            .map_err(|_| invalid("invalid metadata parent"))?;
        let directory = self.listing_directory(parent_path.as_ref())?;
        let before = directory.symlink_metadata(name).map_err(snapshot_io)?;
        reject_cap_reparse(&before)?;
        let after = directory.symlink_metadata(name).map_err(snapshot_io)?;
        let reopened = self
            .listing_directory(parent_path.as_ref())?
            .symlink_metadata(name)
            .map_err(snapshot_io)?;
        if !super::read::same_version(&before, &after)?
            || !super::read::same_version(&before, &reopened)?
            || FolderIdentity::observe(self.identity.root()).map_err(snapshot_io)? != self.identity
        {
            return Err(changed());
        }
        metadata_from(path.clone(), &after)
            .ok_or_else(|| invalid("workspace inspection refuses special nodes"))
    }
    pub(super) fn listing_directory(
        &self,
        path: Option<&WorkspacePath>,
    ) -> Result<Dir, WorkspaceError> {
        path.map_or_else(
            || self.root.try_clone().map_err(snapshot_io),
            |path| self.open_directory(path),
        )
    }
    pub(super) fn scan_directory(
        &self,
        path: Option<&WorkspacePath>,
        storage: Option<&Metadata>,
        mut accept: impl FnMut(DirectoryItem) -> Result<(), WorkspaceError>,
    ) -> Result<(), WorkspaceError> {
        let directory = self.listing_directory(path)?;
        let before = directory.dir_metadata().map_err(snapshot_io)?;
        reject_cap_reparse(&before)?;
        let mut membership = membership::DirectoryMembership::observe(&directory)?;
        for entry in directory.entries().map_err(snapshot_io)? {
            let entry = entry.map_err(snapshot_io)?;
            let name = NativeEntryName::observed(&entry.file_name())?;
            if name.protected() {
                continue;
            }
            membership.visit(&name)?;
            let native = name.native_name()?;
            let observed = directory.symlink_metadata(&native);
            let observed = if let Some(storage) = storage {
                let metadata = observed.map_err(snapshot_io)?;
                if metadata.dev() == storage.dev() && metadata.ino() == storage.ino() {
                    return Err(invalid("directory observation storage aliases a child"));
                }
                Ok(metadata)
            } else {
                observed
            };
            let observation = observed
                .map_or(Err(DirectoryExclusionReason::MetadataUnavailable), |metadata| {
                    observe_child(&directory, path, &name, &native, &metadata)
                });
            accept(DirectoryItem { name, observation })?;
        }
        membership.finish_pass()?;
        membership.verify(&directory)?;
        let reopened_directory = self.listing_directory(path)?;
        membership.verify(&reopened_directory)?;
        let after = directory.dir_metadata().map_err(snapshot_io)?;
        let reopened = reopened_directory.dir_metadata().map_err(snapshot_io)?;
        if !same_directory(&before, &after)?
            || !same_directory(&before, &reopened)?
            || FolderIdentity::observe(self.identity.root()).map_err(snapshot_io)? != self.identity
        {
            return Err(changed());
        }
        Ok(())
    }
}

fn observe_child(
    directory: &Dir,
    parent: Option<&WorkspacePath>,
    name: &NativeEntryName,
    native: &std::ffi::OsStr,
    metadata: &Metadata,
) -> Result<WorkspaceMetadata, DirectoryExclusionReason> {
    if metadata.is_symlink() {
        return Err(DirectoryExclusionReason::SymbolicLink);
    }
    if reject_cap_reparse(metadata).is_err() {
        return Err(DirectoryExclusionReason::ReparsePoint);
    }
    let text = native.to_str().ok_or(DirectoryExclusionReason::UnrepresentableName)?;
    let path = WorkspacePath::new(
        parent.map_or_else(|| text.to_owned(), |parent| format!("{}/{text}", parent.as_str())),
    )
    .map_err(|_| DirectoryExclusionReason::UnrepresentableName)?;
    let result = metadata_from(path, metadata).ok_or(DirectoryExclusionReason::SpecialNode)?;
    let after = directory
        .symlink_metadata(
            name.native_name().map_err(|_| DirectoryExclusionReason::UnrepresentableName)?,
        )
        .map_err(|_| DirectoryExclusionReason::MetadataUnavailable)?;
    if !super::read::same_version(metadata, &after)
        .map_err(|_| DirectoryExclusionReason::MetadataUnavailable)?
    {
        return Err(DirectoryExclusionReason::Changed);
    }
    Ok(result)
}

fn metadata_from(path: WorkspacePath, metadata: &Metadata) -> Option<WorkspaceMetadata> {
    let kind = if metadata.is_file() {
        WorkspaceEntryKind::File
    } else if metadata.is_dir() {
        WorkspaceEntryKind::Directory
    } else {
        return None;
    };
    #[cfg(unix)]
    let executable = {
        use cap_std::fs::PermissionsExt as _;
        kind == WorkspaceEntryKind::File && metadata.permissions().mode() & 0o111 != 0
    };
    #[cfg(not(unix))]
    let executable = false;
    Some(WorkspaceMetadata::inspected(
        path,
        kind,
        if kind == WorkspaceEntryKind::File { metadata.len() } else { 0 },
        executable,
    ))
}
