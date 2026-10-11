//! Bounded, deterministic paging of direct workspace directory entries.

use std::collections::BTreeMap;

use cap_std::{fs::Metadata as CapMetadata, time::SystemTime};
use peritus_patch::WorkspacePath;

use crate::{ErrorCode, FolderIdentity, ReadOnlyWorkspace, RecoveryClass, WorkspaceError};

use super::{
    DirectoryDiagnostic, DirectoryDiagnosticKind, DirectoryEntry, inspect_io, invalid,
    metadata_from_cap,
};

/// Exact opaque continuation for one sorted direct-directory page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryCursor {
    directory: Option<WorkspacePath>,
    after: Vec<u8>,
    version: DirectoryVersion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DirectoryVersion {
    length: u64,
    modified: SystemTime,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    modified_seconds: i64,
    #[cfg(unix)]
    modified_nanoseconds: i64,
    #[cfg(unix)]
    changed_seconds: i64,
    #[cfg(unix)]
    changed_nanoseconds: i64,
    #[cfg(windows)]
    device: u64,
    #[cfg(windows)]
    inode: u64,
    #[cfg(windows)]
    attributes: u32,
    #[cfg(windows)]
    creation_time: u64,
    #[cfg(windows)]
    last_write_time: u64,
}

/// One bounded page of direct children and diagnostics in native-name byte order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryPage {
    entries: Vec<DirectoryEntry>,
    diagnostics: Vec<DirectoryDiagnostic>,
    next: Option<DirectoryCursor>,
}

impl DirectoryPage {
    /// Returns the sorted direct children retained in this page.
    #[must_use]
    pub fn entries(&self) -> &[DirectoryEntry] {
        &self.entries
    }

    /// Unsupported children retained in this page, each reported once across a traversal.
    #[must_use]
    pub fn diagnostics(&self) -> &[DirectoryDiagnostic] {
        &self.diagnostics
    }

    /// Returns a cursor for the next page, or `None` when the directory is exhausted.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<&DirectoryCursor> {
        self.next.as_ref()
    }

    /// Consumes the page and returns its bounded children and continuation.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (Vec<DirectoryEntry>, Vec<DirectoryDiagnostic>, Option<DirectoryCursor>) {
        (self.entries, self.diagnostics, self.next)
    }
}

const DIRECTORY_PAGE_SIZE: usize = 128;

impl ReadOnlyWorkspace {
    /// Reads one canonical child page without retaining a complete wide directory listing.
    ///
    /// Each page selects the next 128 children, including diagnostics, after its opaque cursor from a checked no-follow
    /// directory scan. Directory and workspace identities are rechecked across pages; callers
    /// must resume only with the returned cursor.
    ///
    /// # Errors
    /// Returns a typed failure for invalid cursors, unsafe entries, or directory/workspace drift.
    #[allow(
        clippy::too_many_lines,
        reason = "bounded directory paging validates identity, child diagnostics, and cursor together"
    )]
    pub fn list_directory_page_cancellable(
        &self,
        path: Option<&WorkspacePath>,
        cursor: Option<&DirectoryCursor>,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Option<DirectoryPage>, WorkspaceError> {
        if cursor.is_some_and(|cursor| cursor.directory.as_ref() != path) {
            return Err(invalid("directory cursor belongs to another directory"));
        }
        let directory = self.file_inspection().open_directory(path)?;
        let before = directory.dir_metadata().map_err(|_| inspect_io())?;
        if !before.is_dir() || before.is_symlink() {
            return Err(invalid("workspace inspection target is not a no-follow directory"));
        }
        let version = directory_version(&before)?;
        if cursor.is_some_and(|cursor| cursor.version != version) {
            return Err(changed_directory());
        }
        let root_identity =
            FolderIdentity::observe(self.root()).map_err(|_| changed_directory())?;
        if &root_identity != self.file_inspection().identity() {
            return Err(changed_directory());
        }

        let after = cursor.map(|cursor| cursor.after.as_slice());
        let mut selected = BTreeMap::<Vec<u8>, Result<DirectoryEntry, DirectoryDiagnostic>>::new();
        let retained_limit = DIRECTORY_PAGE_SIZE + 1;
        for entry in directory.entries().map_err(|_| inspect_io())? {
            if cancelled() {
                return Ok(None);
            }
            let entry = entry.map_err(|_| inspect_io())?;
            let raw_name = entry.file_name();
            let key = raw_name.as_encoded_bytes().to_vec();
            if after.is_some_and(|after| key.as_slice() <= after) {
                continue;
            }
            let diagnostic = |kind| DirectoryDiagnostic {
                directory: path.cloned(),
                name_bytes: key.clone(),
                kind,
            };
            let child = match raw_name.into_string() {
                Ok(name) => {
                    if super::protected_component(&name) {
                        continue;
                    }
                    let text =
                        path.map_or_else(|| name.clone(), |parent| format!("{parent}/{name}"));
                    match WorkspacePath::new(text) {
                        Ok(child_path) => {
                            let metadata = entry.metadata().map_err(|_| inspect_io())?;
                            if metadata.is_file() || metadata.is_dir() {
                                Ok(DirectoryEntry(metadata_from_cap(child_path, &metadata)))
                            } else {
                                Err(diagnostic(DirectoryDiagnosticKind::UnsupportedType))
                            }
                        }
                        Err(_) => Err(diagnostic(DirectoryDiagnosticKind::UnsupportedName)),
                    }
                }
                Err(_) => Err(diagnostic(DirectoryDiagnosticKind::UnsupportedName)),
            };
            selected.insert(key, child);
            if selected.len() > retained_limit
                && let Some(last) = selected.keys().next_back().cloned()
            {
                selected.remove(&last);
            }
        }
        if cancelled() {
            return Ok(None);
        }
        let after_metadata = directory.dir_metadata().map_err(|_| inspect_io())?;
        let current_directory = self.file_inspection().open_directory(path)?;
        let current_metadata = current_directory.dir_metadata().map_err(|_| inspect_io())?;
        if directory_version(&after_metadata)? != version
            || directory_version(&current_metadata)? != version
            || FolderIdentity::observe(self.root()).map_err(|_| changed_directory())?
                != root_identity
        {
            return Err(changed_directory());
        }
        let has_more = selected.len() > DIRECTORY_PAGE_SIZE;
        if has_more && let Some(last) = selected.keys().next_back().cloned() {
            selected.remove(&last);
        }
        let next = if has_more {
            let after = selected.keys().next_back().cloned().ok_or_else(changed_directory)?;
            Some(DirectoryCursor { directory: path.cloned(), after, version })
        } else {
            None
        };
        let mut entries = Vec::new();
        let mut diagnostics = Vec::new();
        for child in selected.into_values() {
            match child {
                Ok(entry) => entries.push(entry),
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
        }
        Ok(Some(DirectoryPage { entries, diagnostics, next }))
    }
}

fn directory_version(metadata: &CapMetadata) -> Result<DirectoryVersion, WorkspaceError> {
    Ok(DirectoryVersion {
        length: metadata.len(),
        modified: metadata.modified().map_err(|_| inspect_io())?,
        #[cfg(unix)]
        device: cap_std::fs::MetadataExt::dev(metadata),
        #[cfg(unix)]
        inode: cap_std::fs::MetadataExt::ino(metadata),
        #[cfg(unix)]
        modified_seconds: cap_std::fs::MetadataExt::mtime(metadata),
        #[cfg(unix)]
        modified_nanoseconds: cap_std::fs::MetadataExt::mtime_nsec(metadata),
        #[cfg(unix)]
        changed_seconds: cap_std::fs::MetadataExt::ctime(metadata),
        #[cfg(unix)]
        changed_nanoseconds: cap_std::fs::MetadataExt::ctime_nsec(metadata),
        #[cfg(windows)]
        device: cap_fs_ext::MetadataExt::dev(metadata),
        #[cfg(windows)]
        inode: cap_fs_ext::MetadataExt::ino(metadata),
        #[cfg(windows)]
        attributes: cap_std::fs::MetadataExt::file_attributes(metadata),
        #[cfg(windows)]
        creation_time: cap_std::fs::MetadataExt::creation_time(metadata),
        #[cfg(windows)]
        last_write_time: cap_std::fs::MetadataExt::last_write_time(metadata),
    })
}

const fn changed_directory() -> WorkspaceError {
    super::inspect_error(
        ErrorCode::Indeterminate,
        RecoveryClass::Reobserve,
        "workspace directory changed during paged inspection",
    )
}
