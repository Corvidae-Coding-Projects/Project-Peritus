//! Owner-backed inspection pages bound to one accepted historical observation.

use super::{
    FileReadSelection, FolderInspection, InspectedSelection, InspectionCursor, WorkspaceError,
    changed, invalid, read::same_version, snapshot_io,
};
use cap_fs_ext::MetadataExt as _;
use cap_std::fs::{File, Metadata};
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};
use std::io::{Read as _, Seek as _, SeekFrom};

const PAGE_BYTES: u64 = 64 * 1024;

/// One exact page from retained content, carrying its source interval and explicit continuation.
pub struct InspectionPage {
    observation: InspectedSelection,
    bytes: Vec<u8>,
    range: (u64, u64),
    next: Option<InspectionCursor>,
}
impl std::fmt::Debug for InspectionPage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InspectionPage")
            .field("observation", &self.observation)
            .field("range", &self.range)
            .field("next", &self.next)
            .finish_non_exhaustive()
    }
}
impl InspectionPage {
    /// Borrows complete source and selected-content identity for this page.
    #[must_use]
    pub const fn observation(&self) -> &InspectedSelection {
        &self.observation
    }
    /// Borrows exact original source bytes, including original line terminators.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Returns the actual half-open byte interval in the original source.
    #[must_use]
    pub const fn range(&self) -> (u64, u64) {
        self.range
    }
    /// Returns the next cursor, or None only when the selected content is complete.
    #[must_use]
    pub const fn next(&self) -> Option<InspectionCursor> {
        self.next
    }
}

/// Read capability for exact selected content in caller-owned storage.
///
/// The owner persists the file, observation record, and cursor under its own namespace and
/// publication transaction. This type neither creates ephemeral storage nor expires sessions.
pub struct RetainedInspection {
    file: File,
    observation: InspectedSelection,
    version: Metadata,
}
impl std::fmt::Debug for RetainedInspection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RetainedInspection")
            .field("observation", &self.observation)
            .finish_non_exhaustive()
    }
}
impl FolderInspection {
    /// Captures exact selected bytes in an explicitly owned regular file, then seals a reader.
    /// Storage must be readable, writable, seekable, and distinct from the selected source.
    /// Its prior content is replaced. Failure leaves unaccepted caller-owned storage and
    /// publishes no metadata; the owner is responsible for atomic namespace publication.
    ///
    /// # Errors
    /// Rejects source/output aliasing, unsafe sources, absent selections, observed changes,
    /// invalid storage, and original source/storage I/O failures.
    pub fn capture_file(
        &self,
        path: &WorkspacePath,
        selection: FileReadSelection,
        storage: std::fs::File,
    ) -> Result<RetainedInspection, WorkspaceError> {
        let mut source = self.open_file(path)?;
        let source_metadata = source.metadata().map_err(snapshot_io)?;
        let mut storage = File::from_std(storage);
        let storage_metadata = storage.metadata().map_err(snapshot_io)?;
        if !storage_metadata.is_file()
            || (source_metadata.dev() == storage_metadata.dev()
                && source_metadata.ino() == storage_metadata.ino())
        {
            return Err(invalid("inspection storage must be a distinct regular file"));
        }
        storage.set_len(0).map_err(snapshot_io)?;
        storage.seek(SeekFrom::Start(0)).map_err(snapshot_io)?;
        let observation =
            self.inspect_open_file(path, selection.0, None, &mut source, &mut storage)?;
        storage.sync_all().map_err(snapshot_io)?;
        RetainedInspection::open(storage.into_std(), observation)
    }
}
impl RetainedInspection {
    /// Reopens owner-retained selected content and verifies its complete digest once.
    /// Pages then bind to this opened file version, without re-reading an ambient source.
    ///
    /// # Errors
    /// Rejects non-files, digest/size mismatch, observed storage changes, and original I/O errors.
    pub fn open(
        storage: std::fs::File,
        observation: InspectedSelection,
    ) -> Result<Self, WorkspaceError> {
        let mut file = File::from_std(storage);
        let before = file.metadata().map_err(snapshot_io)?;
        if !before.is_file() || before.len() != observation.selected_bytes() {
            return Err(invalid(
                "retained inspection size or file type differs from its observation",
            ));
        }
        file.seek(SeekFrom::Start(0)).map_err(snapshot_io)?;
        let mut digest = Sha256::new();
        let mut count = 0_u64;
        let mut chunk = vec![0_u8; 64 * 1024];
        loop {
            let read = file.read(&mut chunk).map_err(snapshot_io)?;
            if read == 0 {
                break;
            }
            count = count.checked_add(read as u64).ok_or_else(changed)?;
            if count > before.len() {
                return Err(changed());
            }
            digest.update(&chunk[..read]);
        }
        if count != before.len()
            || Sha256Digest::new(digest.finalize().into()) != observation.digest()
            || !same_version(&before, &file.metadata().map_err(snapshot_io)?)?
        {
            return Err(changed());
        }
        Ok(Self { file, observation, version: before })
    }
    /// Borrows metadata that the owner can encode and persist alongside retained content.
    #[must_use]
    pub const fn observation(&self) -> &InspectedSelection {
        &self.observation
    }
    /// Returns the initial cursor, or None for an empty selection.
    #[must_use]
    pub fn cursor(&self) -> Option<InspectionCursor> {
        (self.observation.selected_bytes() != 0)
            .then(|| InspectionCursor { observation: self.observation.binding(), offset: 0 })
    }
    /// Reads a page of at most the caller's nonzero capacity using fixed working memory.
    /// A physical page is at most 64 KiB and always carries an explicit continuation when
    /// more content remains. Page count and total included content have no ceiling.
    /// Retrying a cursor reads the same interval; failure never advances the caller's cursor.
    ///
    /// # Errors
    /// Rejects zero capacity, foreign/out-of-range cursors, changed retained storage, and I/O errors.
    pub fn read_page(
        &mut self,
        cursor: InspectionCursor,
        maximum_bytes: u64,
    ) -> Result<Option<InspectionPage>, WorkspaceError> {
        if maximum_bytes == 0
            || cursor.observation != self.observation.binding()
            || cursor.offset > self.observation.selected_bytes()
        {
            return Err(invalid("invalid retained inspection cursor or page capacity"));
        }
        if !same_version(&self.version, &self.file.metadata().map_err(snapshot_io)?)? {
            return Err(changed());
        }
        let available = self.observation.selected_bytes() - cursor.offset;
        if available == 0 {
            return Ok(None);
        }
        let count = usize::try_from(available.min(maximum_bytes).min(PAGE_BYTES))
            .map_err(|_| invalid("page size cannot be represented"))?;
        self.file.seek(SeekFrom::Start(cursor.offset)).map_err(snapshot_io)?;
        let mut bytes = vec![0; count];
        self.file.read_exact(&mut bytes).map_err(snapshot_io)?;
        if !same_version(&self.version, &self.file.metadata().map_err(snapshot_io)?)? {
            return Err(changed());
        }
        let offset = cursor.offset + count as u64;
        let start = self.observation.range().0 + cursor.offset;
        let end = self.observation.range().0 + offset;
        let next = (offset < self.observation.selected_bytes())
            .then_some(InspectionCursor { observation: cursor.observation, offset });
        Ok(Some(InspectionPage {
            observation: self.observation.clone(),
            bytes,
            range: (start, end),
            next,
        }))
    }
}
