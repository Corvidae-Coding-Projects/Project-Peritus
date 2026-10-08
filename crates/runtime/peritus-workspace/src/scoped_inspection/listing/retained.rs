//! Owned accepted listing and verified continuation, independent of ambient directory changes.

use super::super::{
    FolderInspection, WorkspaceError, changed, invalid, read::same_version, snapshot_io,
};
use super::{
    DirectoryCursor, DirectoryItem, ObservedDirectory,
    record::platform_tag,
    storage::{BodyReader, BodyWriter},
};
use cap_std::fs::{File, Metadata};
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};
use std::io::{Seek as _, SeekFrom, Write as _};

const MAGIC: &[u8; 8] = b"PDIRv001";
// Physical response window only: every nonfinal page returns a continuation.
const PAGE_ITEMS: u64 = 256;

/// Exact native children from one retained listing, including typed exclusions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryPage {
    observation: ObservedDirectory,
    start: u64,
    items: Vec<DirectoryItem>,
    next: Option<DirectoryCursor>,
}
impl DirectoryPage {
    /// Borrows the complete accepted listing identity.
    #[must_use]
    pub const fn observation(&self) -> &ObservedDirectory {
        &self.observation
    }
    /// Returns the first zero-based native child ordinal in this page.
    #[must_use]
    pub const fn start(&self) -> u64 {
        self.start
    }
    /// Borrows exact native children and exclusions.
    #[must_use]
    pub fn items(&self) -> &[DirectoryItem] {
        &self.items
    }
    /// Returns explicit continuation, or `None` only when the accepted listing is complete.
    #[must_use]
    pub const fn next(&self) -> Option<DirectoryCursor> {
        self.next
    }
}

/// Reader for an owner-published listing body, metadata and continuation.
///
/// The owner supplies the durable namespace and atomically publishes a fully verified root.
/// This type creates no ephemeral storage, session expiration or cumulative entry allowance.
pub struct RetainedDirectory {
    file: File,
    observation: ObservedDirectory,
    version: Metadata,
    cursor: Option<DirectoryCursor>,
}
impl std::fmt::Debug for RetainedDirectory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RetainedDirectory")
            .field("observation", &self.observation)
            .field("cursor", &self.cursor)
            .finish_non_exhaustive()
    }
}
impl FolderInspection {
    /// Captures exposed native children and exclusions into an empty owner-owned regular file.
    /// Storage must be readable/writable/seekable and must not alias any source child.
    /// Existing accepted storage is never overwritten. Failure leaves an unaccepted prefix;
    /// only successful verification yields metadata for the owner's atomic publication.
    ///
    /// # Errors
    /// Rejects unsafe/changed directories, nonempty/aliased storage, incomplete alias proof,
    /// incomplete iteration, observed root changes and original storage/iteration I/O errors.
    pub fn capture_directory(
        &self,
        path: Option<&WorkspacePath>,
        storage: std::fs::File,
    ) -> Result<RetainedDirectory, WorkspaceError> {
        self.capture_directory_with_cancel(path, storage, || false)
    }

    /// Captures exact children like [`Self::capture_directory`], while checking an owner-supplied
    /// cancellation predicate between native directory reads. Cancellation leaves only an
    /// unaccepted storage prefix and never publishes a cursor or observation.
    ///
    /// # Errors
    /// Rejects cancellation, unsafe/changed directories, nonempty/aliased storage, incomplete
    /// iteration, observed root changes and original storage/iteration I/O errors.
    pub fn capture_directory_with_cancel(
        &self,
        path: Option<&WorkspacePath>,
        storage: std::fs::File,
        mut should_cancel: impl FnMut() -> bool,
    ) -> Result<RetainedDirectory, WorkspaceError> {
        let mut file = File::from_std(storage);
        let before = file.metadata().map_err(snapshot_io)?;
        if !before.is_file() || before.len() != 0 {
            return Err(invalid("directory storage must be an empty owned regular file"));
        }
        let expected = self.stream_directory(path, Some(&before), &mut should_cancel, |_| Ok(()))?;
        file.seek(SeekFrom::Start(0)).map_err(snapshot_io)?;
        let mut body = BodyWriter { file: &mut file, position: 0, hasher: Sha256::new() };
        body.write_all(MAGIC).map_err(snapshot_io)?;
        body.write_all(&[platform_tag()]).map_err(snapshot_io)?;
        let mut count = 0_u64;
        let observed = self.stream_directory(path, Some(&before), &mut should_cancel, |item| {
            body.item(&item, count)?;
            count = count.checked_add(1).ok_or_else(changed)?;
            Ok(())
        })?;
        if observed != expected || observed.count != count {
            return Err(changed());
        }
        let bytes = body.position;
        let digest = Sha256Digest::new(body.hasher.finalize().into());
        file.sync_all().map_err(snapshot_io)?;
        let observation = ObservedDirectory {
            folder: self.identity.digest(),
            path: path.cloned(),
            count,
            bytes,
            digest,
        };
        RetainedDirectory::open_with_cancel(file.into_std(), observation, None, should_cancel)
    }
}
impl RetainedDirectory {
    /// Reopens and verifies the full accepted body and an optional exact resume boundary.
    /// Ambient sources are never reread. Reopening with an older cursor permits exact replay.
    ///
    /// # Errors
    /// Rejects changed/corrupt bodies, foreign/malformed cursors, invalid record boundaries,
    /// unsupported versions/platforms, invalid storage and original I/O failures.
    pub fn open(
        storage: std::fs::File,
        observation: ObservedDirectory,
        resume: Option<DirectoryCursor>,
    ) -> Result<Self, WorkspaceError> {
        Self::open_with_cancel(storage, observation, resume, || false)
    }

    /// Reopens and verifies an accepted body while checking cancellation between retained-record
    /// reads. The accepted body and cursor remain reusable when cancellation interrupts recovery.
    ///
    /// # Errors
    /// Rejects cancellation, changed/corrupt bodies, foreign/malformed cursors, invalid record
    /// boundaries, unsupported versions/platforms, invalid storage and original I/O failures.
    pub fn open_with_cancel(
        storage: std::fs::File,
        observation: ObservedDirectory,
        resume: Option<DirectoryCursor>,
        mut should_cancel: impl FnMut() -> bool,
    ) -> Result<Self, WorkspaceError> {
        if should_cancel() {
            return Err(super::cancelled());
        }
        let mut file = File::from_std(storage);
        let version = file.metadata().map_err(snapshot_io)?;
        if !version.is_file() || version.len() != observation.bytes {
            return Err(invalid("retained directory size differs from its observation"));
        }
        let initial = DirectoryCursor { binding: observation.binding(), index: 0, offset: 9 };
        let cursor = resume.unwrap_or(initial);
        if cursor.binding != initial.binding {
            return Err(invalid("directory cursor belongs to another observation"));
        }
        if cursor.index > observation.count {
            return Err(invalid("directory cursor ordinal is beyond the listing"));
        }
        if cursor.offset > observation.bytes {
            return Err(invalid("directory cursor offset is beyond retained bytes"));
        }
        file.seek(SeekFrom::Start(0)).map_err(snapshot_io)?;
        let mut body = BodyReader {
            file: &mut file,
            position: 0,
            end: observation.bytes,
            hasher: Some(Sha256::new()),
        };
        if &body.fixed::<8>()? != MAGIC || body.fixed::<1>()?[0] != platform_tag() {
            return Err(invalid("retained directory version or platform is unsupported"));
        }
        let mut boundary_valid = false;
        for index in 0..observation.count {
            if should_cancel() {
                return Err(super::cancelled());
            }
            if index == cursor.index {
                boundary_valid = body.position == cursor.offset;
            }
            body.item(&observation, index)?;
        }
        if should_cancel() {
            return Err(super::cancelled());
        }
        if cursor.index == observation.count {
            boundary_valid = body.position == cursor.offset;
        }
        if !boundary_valid || body.position != observation.bytes {
            return Err(invalid("directory cursor is not the claimed record boundary"));
        }
        let digest = Sha256Digest::new(
            body.hasher.take().ok_or_else(|| invalid("missing listing digest"))?.finalize().into(),
        );
        if digest != observation.digest
            || !same_version(&version, &file.metadata().map_err(snapshot_io)?)?
        {
            return Err(changed());
        }
        let cursor = (cursor.index < observation.count).then_some(cursor);
        Ok(Self { file, observation, version, cursor })
    }
    /// Borrows owner-persistable accepted listing metadata.
    #[must_use]
    pub const fn observation(&self) -> &ObservedDirectory {
        &self.observation
    }
    /// Returns the next exact cursor, or `None` when the accepted listing is complete.
    #[must_use]
    pub const fn cursor(&self) -> Option<DirectoryCursor> {
        self.cursor
    }
    /// Reads a physical window of native children without a total count or page budget.
    /// A failed read does not advance the accepted cursor. Only the current validated cursor
    /// is accepted; reopen with a saved cursor to replay or resume an earlier position.
    ///
    /// # Errors
    /// Rejects zero capacity, stale/foreign cursors, changed storage and original I/O failures.
    pub fn read_page(
        &mut self,
        cursor: DirectoryCursor,
        maximum_items: u64,
    ) -> Result<DirectoryPage, WorkspaceError> {
        if maximum_items == 0 || Some(cursor) != self.cursor {
            return Err(invalid("directory cursor or page capacity is invalid"));
        }
        if !same_version(&self.version, &self.file.metadata().map_err(snapshot_io)?)? {
            return Err(changed());
        }
        self.file.seek(SeekFrom::Start(cursor.offset)).map_err(snapshot_io)?;
        let mut body = BodyReader {
            file: &mut self.file,
            position: cursor.offset,
            end: self.observation.bytes,
            hasher: None,
        };
        let count = maximum_items.min(PAGE_ITEMS).min(self.observation.count - cursor.index);
        let mut items = Vec::new();
        for index in cursor.index..cursor.index + count {
            items.push(body.item(&self.observation, index)?);
        }
        let offset = body.position;
        if !same_version(&self.version, &self.file.metadata().map_err(snapshot_io)?)? {
            return Err(changed());
        }
        let index = cursor.index + count;
        let next = (index < self.observation.count).then_some(DirectoryCursor {
            binding: cursor.binding,
            index,
            offset,
        });
        self.cursor = next;
        Ok(DirectoryPage {
            observation: self.observation.clone(),
            start: cursor.index,
            items,
            next,
        })
    }
}
