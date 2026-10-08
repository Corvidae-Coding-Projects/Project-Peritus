//! Owner-retained, replayable direct-directory pages for workspace navigation.

use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File},
    io::Write as _,
    path::{Path, PathBuf},
    sync::{
        Arc,
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use peritus_patch::WorkspacePath;
use peritus_provider_core::CancellationToken;
use peritus_types::Sha256Digest;
use peritus_workspace::{
    DirectoryCursor, DirectoryItem, FolderIdentity, FolderInspection, ObservedDirectory,
    RetainedDirectory, WorkspaceError,
};

const TOKEN_MAGIC: &[u8; 8] = b"PWDLv001";
const DIRECTORY_PAGE_ITEMS: u64 = 256;

/// One exact physical page from a complete owner-retained direct-directory observation.
pub(crate) struct RetainedListingPage {
    observation: ObservedDirectory,
    start: u64,
    items: Vec<DirectoryItem>,
    replay: Option<String>,
    next: Option<String>,
}

impl RetainedListingPage {
    pub(crate) const fn observation(&self) -> &ObservedDirectory {
        &self.observation
    }

    pub(crate) const fn start(&self) -> u64 {
        self.start
    }

    pub(crate) fn items(&self) -> &[DirectoryItem] {
        &self.items
    }

    pub(crate) fn observation_handle(&self) -> String {
        digest_hex(peritus_codec::sha256(&self.observation.to_record()))
    }

    pub(crate) fn replay(&self) -> Option<&str> {
        self.replay.as_deref()
    }

    pub(crate) fn next(&self) -> Option<&str> {
        self.next.as_deref()
    }

    pub(crate) const fn complete(&self) -> bool {
        self.next.is_none()
    }
}

/// Durable namespace and cancellation ownership for accepted listing bodies.
pub(crate) struct DirectoryListingOwner {
    workspace_root: PathBuf,
    storage_root: PathBuf,
    cancelled: Arc<AtomicBool>,
    provider_cancellation: CancellationToken,
    active: Mutex<BTreeMap<String, RetainedDirectory>>,
    _temporary: Option<tempfile::TempDir>,
}

impl DirectoryListingOwner {
    pub(crate) fn new(
        workspace_root: PathBuf,
        storage_root: PathBuf,
        cancelled: Arc<AtomicBool>,
        provider_cancellation: CancellationToken,
    ) -> Self {
        Self {
            workspace_root,
            storage_root,
            cancelled,
            provider_cancellation,
            active: Mutex::new(BTreeMap::new()),
            _temporary: None,
        }
    }

    pub(crate) fn ephemeral(workspace_root: PathBuf) -> Result<Self, std::io::Error> {
        let temporary = tempfile::Builder::new()
            .prefix("peritus-directory-listings-")
            .tempdir()?;
        let storage_root = temporary.path().to_path_buf();
        Ok(Self {
            workspace_root,
            storage_root,
            cancelled: Arc::new(AtomicBool::new(false)),
            provider_cancellation: CancellationToken::new(),
            active: Mutex::new(BTreeMap::new()),
            _temporary: Some(temporary),
        })
    }

    pub(crate) fn page(
        &self,
        path: Option<&WorkspacePath>,
        cursor: Option<&str>,
    ) -> Result<RetainedListingPage, ListingError> {
        if self.is_cancelled() {
            return Err(ListingError::Cancelled);
        }
        fs::create_dir_all(&self.storage_root).map_err(ListingError::Storage)?;
        if let Some(cursor) = cursor {
            return self.resume(path, cursor);
        }
        self.capture(path)
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) || self.provider_cancellation.is_cancelled()
    }

    fn capture(
        &self,
        path: Option<&WorkspacePath>,
    ) -> Result<RetainedListingPage, ListingError> {
        let identity = FolderIdentity::observe(&self.workspace_root).map_err(ListingError::Storage)?;
        let inspection = FolderInspection::open(&identity).map_err(|error| self.workspace(error))?;
        let candidate = tempfile::NamedTempFile::new_in(&self.storage_root)
            .map_err(ListingError::Storage)?;
        let storage = candidate.reopen().map_err(ListingError::Storage)?;
        let retained = inspection
            .capture_directory_with_cancel(path, storage, || self.is_cancelled())
            .map_err(|error| self.workspace(error))?;
        let observation = retained.observation().clone();
        let body_path = self.body_path(&observation);
        let retained = match candidate.persist_noclobber(&body_path) {
            Ok(_) => {
                sync_storage_directory(&self.storage_root).map_err(ListingError::Storage)?;
                retained
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                let file = File::open(&body_path).map_err(ListingError::Storage)?;
                RetainedDirectory::open_with_cancel(file, observation.clone(), None, || {
                    self.is_cancelled()
                })
                    .map_err(|error| self.workspace(error))?
            }
            Err(error) => return Err(ListingError::Storage(error.error)),
        };
        self.publish_observation(&observation)?;
        self.read_and_retain(retained)
    }

    fn resume(
        &self,
        path: Option<&WorkspacePath>,
        cursor: &str,
    ) -> Result<RetainedListingPage, ListingError> {
        let (binding, cursor) = decode_cursor(cursor)?;
        let observation = self.read_observation(binding)?;
        if observation.path() != path {
            return Err(ListingError::Invalid(
                "directory continuation belongs to another workspace path",
            ));
        }
        let identity = FolderIdentity::observe(&self.workspace_root).map_err(ListingError::Storage)?;
        if observation.folder() != identity.digest() {
            return Err(ListingError::Invalid(
                "directory continuation belongs to another workspace identity",
            ));
        }
        let key = digest_hex(peritus_codec::sha256(&observation.to_record()));
        {
            let mut active = self
                .active
                .lock()
                .map_err(|_| ListingError::Invalid("directory navigation ownership is poisoned"))?;
            if active.get(&key).is_some_and(|retained| retained.cursor() == Some(cursor)) {
                let page = self.read_page(
                    active
                        .get_mut(&key)
                        .ok_or(ListingError::Invalid("active directory listing disappeared"))?,
                )?;
                if page.complete() {
                    active.remove(&key);
                }
                return Ok(page);
            }
        }
        let file = File::open(self.body_path(&observation)).map_err(ListingError::Storage)?;
        let retained = RetainedDirectory::open_with_cancel(file, observation, Some(cursor), || {
            self.is_cancelled()
        })
            .map_err(|error| self.workspace(error))?;
        self.read_and_retain(retained)
    }

    fn read_and_retain(
        &self,
        mut retained: RetainedDirectory,
    ) -> Result<RetainedListingPage, ListingError> {
        let key = digest_hex(peritus_codec::sha256(&retained.observation().to_record()));
        let page = self.read_page(&mut retained)?;
        if !page.complete() {
            self.active
                .lock()
                .map_err(|_| ListingError::Invalid("directory navigation ownership is poisoned"))?
                .insert(key, retained);
        }
        Ok(page)
    }

    fn read_page(
        &self,
        retained: &mut RetainedDirectory,
    ) -> Result<RetainedListingPage, ListingError> {
        let observation = retained.observation().clone();
        let Some(cursor) = retained.cursor() else {
            return Ok(RetainedListingPage {
                observation,
                start: 0,
                items: Vec::new(),
                replay: None,
                next: None,
            });
        };
        if self.is_cancelled() {
            return Err(ListingError::Cancelled);
        }
        let replay = Some(encode_cursor(&observation, cursor));
        let page = retained
            .read_page(cursor, DIRECTORY_PAGE_ITEMS)
            .map_err(|error| self.workspace(error))?;
        let next = page.next().map(|next| encode_cursor(&observation, next));
        Ok(RetainedListingPage {
            observation,
            start: page.start(),
            items: page.items().to_vec(),
            replay,
            next,
        })
    }

    fn body_path(&self, observation: &ObservedDirectory) -> PathBuf {
        let binding = peritus_codec::sha256(&observation.to_record());
        self.storage_root.join(format!("{}.directory", digest_hex(binding)))
    }

    fn record_path(&self, binding: Sha256Digest) -> PathBuf {
        self.storage_root.join(format!("{}.record", digest_hex(binding)))
    }

    fn publish_observation(&self, observation: &ObservedDirectory) -> Result<(), ListingError> {
        let record = observation.to_record();
        let binding = peritus_codec::sha256(&record);
        let path = self.record_path(binding);
        let mut candidate = tempfile::NamedTempFile::new_in(&self.storage_root)
            .map_err(ListingError::Storage)?;
        candidate.write_all(&record).map_err(ListingError::Storage)?;
        candidate.as_file().sync_all().map_err(ListingError::Storage)?;
        match candidate.persist_noclobber(&path) {
            Ok(_) => sync_storage_directory(&self.storage_root).map_err(ListingError::Storage),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                let retained = fs::read(path).map_err(ListingError::Storage)?;
                if retained == record {
                    Ok(())
                } else {
                    Err(ListingError::Invalid(
                        "retained directory observation record conflicts with its binding",
                    ))
                }
            }
            Err(error) => Err(ListingError::Storage(error.error)),
        }
    }

    fn read_observation(
        &self,
        binding: Sha256Digest,
    ) -> Result<ObservedDirectory, ListingError> {
        let record = fs::read(self.record_path(binding)).map_err(ListingError::Storage)?;
        if peritus_codec::sha256(&record) != binding {
            return Err(ListingError::Invalid(
                "retained directory observation differs from its handle",
            ));
        }
        ObservedDirectory::from_record(&record).map_err(ListingError::Workspace)
    }

    fn workspace(&self, error: WorkspaceError) -> ListingError {
        if self.is_cancelled() {
            ListingError::Cancelled
        } else {
            ListingError::Workspace(error)
        }
    }
}

#[derive(Debug)]
pub(crate) enum ListingError {
    Cancelled,
    Invalid(&'static str),
    Storage(std::io::Error),
    Workspace(WorkspaceError),
}

impl fmt::Display for ListingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("directory navigation was cancelled"),
            Self::Invalid(detail) => formatter.write_str(detail),
            Self::Storage(error) => write!(formatter, "directory navigation storage: {error}"),
            Self::Workspace(error) => write!(formatter, "directory navigation: {error}"),
        }
    }
}

impl std::error::Error for ListingError {}

fn encode_cursor(observation: &ObservedDirectory, cursor: DirectoryCursor) -> String {
    let binding = peritus_codec::sha256(&observation.to_record());
    let cursor = cursor.to_record();
    let mut bytes = TOKEN_MAGIC.to_vec();
    bytes.extend_from_slice(binding.as_bytes());
    bytes.extend_from_slice(&(cursor.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&cursor);
    let checksum = peritus_codec::sha256(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    encode_hex(&bytes)
}

fn decode_cursor(value: &str) -> Result<(Sha256Digest, DirectoryCursor), ListingError> {
    let bytes = decode_hex(value)?;
    let payload_length = bytes
        .len()
        .checked_sub(32)
        .ok_or(ListingError::Invalid("directory continuation is incomplete"))?;
    let (payload, checksum) = bytes.split_at(payload_length);
    if !payload.starts_with(TOKEN_MAGIC)
        || peritus_codec::sha256(payload).as_bytes() != checksum
    {
        return Err(ListingError::Invalid(
            "directory continuation version or checksum is invalid",
        ));
    }
    let mut remaining = &payload[TOKEN_MAGIC.len()..];
    let binding = Sha256Digest::new(
        take_bytes(&mut remaining, 32)?
            .try_into()
            .map_err(|_| ListingError::Invalid("directory observation handle is incomplete"))?,
    );
    let cursor_length = take_length(&mut remaining)?;
    let cursor = take_bytes(&mut remaining, cursor_length)?;
    if !remaining.is_empty() {
        return Err(ListingError::Invalid("directory continuation has trailing data"));
    }
    Ok((
        binding,
        DirectoryCursor::from_record(cursor).map_err(ListingError::Workspace)?,
    ))
}

fn take_length(bytes: &mut &[u8]) -> Result<usize, ListingError> {
    let encoded = take_bytes(bytes, 8)?;
    usize::try_from(u64::from_be_bytes(
        encoded
            .try_into()
            .map_err(|_| ListingError::Invalid("directory continuation length is incomplete"))?,
    ))
    .map_err(|_| ListingError::Invalid("directory continuation length is not representable"))
}

fn take_bytes<'a>(bytes: &mut &'a [u8], length: usize) -> Result<&'a [u8], ListingError> {
    let value = bytes
        .get(..length)
        .ok_or(ListingError::Invalid("directory continuation field is incomplete"))?;
    *bytes = &bytes[length..];
    Ok(value)
}

fn encode_hex(bytes: &[u8]) -> String {
    use fmt::Write as _;
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn decode_hex(value: &str) -> Result<Vec<u8>, ListingError> {
    if !value.len().is_multiple_of(2) {
        return Err(ListingError::Invalid("directory continuation is not hexadecimal"));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_digit(pair[0])?;
            let low = hex_digit(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

const fn hex_digit(byte: u8) -> Result<u8, ListingError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ListingError::Invalid("directory continuation is not hexadecimal")),
    }
}

fn digest_hex(digest: Sha256Digest) -> String {
    encode_hex(digest.as_bytes())
}

#[cfg(unix)]
fn sync_storage_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_storage_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
