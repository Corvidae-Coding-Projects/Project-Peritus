//! Durable whole-source captures and exact physical pages for workspace reads.

use std::{
    fmt,
    fmt::Write as _,
    fs::{self, File},
    io::Write as _,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use peritus_patch::WorkspacePath;
use peritus_provider_core::CancellationToken;
use peritus_types::Sha256Digest;
use peritus_workspace::{
    FileReadSelection, FolderIdentity, FolderInspection, InspectedSelection, RetainedInspection,
    WorkspaceError,
};

const READ_RECORD_MAGIC: &[u8; 8] = b"PWRDv001";
const READ_CURSOR_MAGIC: &[u8; 8] = b"PWRCv001";
const READ_PAGE_BYTES: u64 = 16 * 1024;
const SCAN_PAGE_BYTES: u64 = 64 * 1024;

/// Durable namespace and cancellation ownership for retained workspace inspections.
pub(crate) struct WorkspaceInspectionOwner {
    pub(super) workspace_root: PathBuf,
    pub(super) storage_root: PathBuf,
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) provider_cancellation: CancellationToken,
    _temporary: Option<tempfile::TempDir>,
}

impl WorkspaceInspectionOwner {
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
            _temporary: None,
        }
    }

    pub(crate) fn ephemeral(workspace_root: PathBuf) -> Result<Self, std::io::Error> {
        let temporary = tempfile::Builder::new()
            .prefix("peritus-workspace-inspections-")
            .tempdir()?;
        let storage_root = temporary.path().to_path_buf();
        Ok(Self {
            workspace_root,
            storage_root,
            cancelled: Arc::new(AtomicBool::new(false)),
            provider_cancellation: CancellationToken::new(),
            _temporary: Some(temporary),
        })
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) || self.provider_cancellation.is_cancelled()
    }

    pub(crate) fn read_page(
        &self,
        path: &WorkspacePath,
        first_line: u64,
        last_line: Option<u64>,
        cursor: Option<&str>,
    ) -> Result<RetainedReadPage, InspectionError> {
        if self.is_cancelled() {
            return Err(InspectionError::Cancelled);
        }
        if first_line == 0 || last_line.is_some_and(|last| last < first_line) {
            return Err(InspectionError::Invalid(
                "workspace read line bounds are invalid".to_owned(),
            ));
        }
        fs::create_dir_all(&self.storage_root).map_err(InspectionError::Storage)?;
        let request = read_request_binding(path, first_line, last_line);
        let (binding, offset) = if let Some(cursor) = cursor {
            let decoded = decode_read_cursor(cursor)?;
            if decoded.request != request {
                return Err(InspectionError::Invalid(
                    "workspace read continuation belongs to another path or line selection"
                        .to_owned(),
                ));
            }
            (decoded.observation, decoded.offset)
        } else {
            (self.capture_read(path)?, u64::MAX)
        };
        let record = self.read_record(binding)?;
        if record.observation.path() != path {
            return Err(InspectionError::Invalid(
                "workspace read continuation belongs to another source path".to_owned(),
            ));
        }
        let identity =
            FolderIdentity::observe(&self.workspace_root).map_err(InspectionError::Storage)?;
        if record.observation.folder_digest() != identity.digest() {
            return Err(InspectionError::Invalid(
                "workspace read continuation belongs to another workspace identity".to_owned(),
            ));
        }
        let file = File::open(self.read_body_path(binding)).map_err(InspectionError::Storage)?;
        let mut retained =
            RetainedInspection::open(file, record.observation.clone()).map_err(InspectionError::Workspace)?;
        let selection = resolve_line_selection(&mut retained, first_line, last_line)?;
        let offset = if cursor.is_some() { offset } else { selection.start };
        if offset < selection.start || offset > selection.end {
            return Err(InspectionError::Invalid(
                "workspace read continuation is outside its selected line range".to_owned(),
            ));
        }
        let handle = digest_hex(read_observation_handle(binding, request));
        if selection.start == selection.end {
            if cursor.is_some() {
                return Err(InspectionError::Invalid(
                    "completed workspace read has no continuation".to_owned(),
                ));
            }
            return Ok(RetainedReadPage {
                observation: record.observation,
                observation_handle: handle,
                permissions: record.permissions,
                selection,
                range: (offset, offset),
                content: String::new(),
                start_position: None,
                end_position: None,
                last_rendered_line: None,
                replay: None,
                next: None,
            });
        }
        if offset == selection.end {
            return Err(InspectionError::Invalid(
                "completed workspace read has no continuation".to_owned(),
            ));
        }
        let start_position =
            position_at(&mut retained, selection.start, offset, first_line)?;
        let maximum = READ_PAGE_BYTES.min(selection.end - offset);
        let (bytes, range) = read_utf8_page(&mut retained, offset, maximum)?;
        let end_position = advance_position(start_position, &bytes)?;
        let last_rendered_line = last_rendered_line(start_position.line, &bytes);
        let content = render_numbered(start_position.line, &bytes)?;
        let replay = Some(encode_read_cursor(binding, request, offset));
        let next = (range.1 < selection.end)
            .then(|| encode_read_cursor(binding, request, range.1));
        Ok(RetainedReadPage {
            observation: record.observation,
            observation_handle: handle,
            permissions: record.permissions,
            selection,
            range,
            content,
            start_position: Some(start_position),
            end_position: Some(end_position),
            last_rendered_line,
            replay,
            next,
        })
    }

    fn capture_read(&self, path: &WorkspacePath) -> Result<Sha256Digest, InspectionError> {
        let identity =
            FolderIdentity::observe(&self.workspace_root).map_err(InspectionError::Storage)?;
        let inspection =
            FolderInspection::open(&identity).map_err(InspectionError::Workspace)?;
        let metadata = fs::symlink_metadata(self.workspace_root.join(path.as_path()))
            .map_err(InspectionError::Storage)?;
        let permissions = crate::file_metadata::permissions(&metadata);
        let candidate =
            tempfile::NamedTempFile::new_in(&self.storage_root).map_err(InspectionError::Storage)?;
        let storage = candidate.reopen().map_err(InspectionError::Storage)?;
        let retained = inspection
            .capture_file(path, FileReadSelection::all(), storage)
            .map_err(InspectionError::Workspace)?;
        let record = ReadRecord {
            observation: retained.observation().clone(),
            permissions,
        };
        drop(retained);
        let encoded = record.encode();
        let binding = peritus_codec::sha256(&encoded);
        let body_path = self.read_body_path(binding);
        match candidate.persist_noclobber(&body_path) {
            Ok(_) => sync_storage_directory(&self.storage_root).map_err(InspectionError::Storage)?,
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                let file = File::open(&body_path).map_err(InspectionError::Storage)?;
                RetainedInspection::open(file, record.observation.clone())
                    .map_err(InspectionError::Workspace)?;
            }
            Err(error) => return Err(InspectionError::Storage(error.error)),
        }
        self.publish_record(binding, &encoded, "read")?;
        Ok(binding)
    }

    fn read_record(&self, binding: Sha256Digest) -> Result<ReadRecord, InspectionError> {
        let bytes =
            fs::read(self.record_path(binding, "read")).map_err(InspectionError::Storage)?;
        if peritus_codec::sha256(&bytes) != binding {
            return Err(InspectionError::Invalid(
                "retained workspace read record differs from its handle".to_owned(),
            ));
        }
        ReadRecord::decode(&bytes)
    }

    pub(super) fn publish_record(
        &self,
        binding: Sha256Digest,
        bytes: &[u8],
        suffix: &str,
    ) -> Result<(), InspectionError> {
        let path = self.record_path(binding, suffix);
        let mut candidate =
            tempfile::NamedTempFile::new_in(&self.storage_root).map_err(InspectionError::Storage)?;
        candidate.write_all(bytes).map_err(InspectionError::Storage)?;
        candidate.as_file().sync_all().map_err(InspectionError::Storage)?;
        match candidate.persist_noclobber(&path) {
            Ok(_) => sync_storage_directory(&self.storage_root).map_err(InspectionError::Storage),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                let retained = fs::read(path).map_err(InspectionError::Storage)?;
                if retained == bytes {
                    Ok(())
                } else {
                    Err(InspectionError::Invalid(
                        "retained inspection record conflicts with its binding".to_owned(),
                    ))
                }
            }
            Err(error) => Err(InspectionError::Storage(error.error)),
        }
    }

    pub(super) fn record_path(&self, binding: Sha256Digest, suffix: &str) -> PathBuf {
        self.storage_root.join(format!("{}.{}.record", digest_hex(binding), suffix))
    }

    fn read_body_path(&self, binding: Sha256Digest) -> PathBuf {
        self.storage_root.join(format!("{}.read", digest_hex(binding)))
    }
}

pub(crate) struct RetainedReadPage {
    pub(super) observation: InspectedSelection,
    pub(super) observation_handle: String,
    pub(super) permissions: String,
    pub(super) selection: ResolvedSelection,
    pub(super) range: (u64, u64),
    pub(super) content: String,
    pub(super) start_position: Option<SourcePosition>,
    pub(super) end_position: Option<SourcePosition>,
    pub(super) last_rendered_line: Option<u64>,
    pub(super) replay: Option<String>,
    pub(super) next: Option<String>,
}

#[derive(Clone, Copy)]
pub(super) struct ResolvedSelection {
    pub(super) start: u64,
    pub(super) end: u64,
}

#[derive(Clone, Copy)]
pub(super) struct SourcePosition {
    pub(super) line: u64,
    pub(super) column_bytes: u64,
}

struct ReadRecord {
    observation: InspectedSelection,
    permissions: String,
}

impl ReadRecord {
    fn encode(&self) -> Vec<u8> {
        let observation = self.observation.encode();
        let mut bytes = READ_RECORD_MAGIC.to_vec();
        bytes.extend_from_slice(&(observation.len() as u64).to_be_bytes());
        bytes.extend_from_slice(&observation);
        bytes.extend_from_slice(&(self.permissions.len() as u64).to_be_bytes());
        bytes.extend_from_slice(self.permissions.as_bytes());
        seal(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, InspectionError> {
        let mut payload = checked_record(bytes, READ_RECORD_MAGIC, "workspace read")?;
        let observation_length = take_length(&mut payload, "workspace read observation")?;
        let observation = take_bytes(&mut payload, observation_length, "workspace read observation")?;
        let observation =
            InspectedSelection::decode(observation).map_err(InspectionError::Workspace)?;
        let permissions_length = take_length(&mut payload, "workspace read permissions")?;
        let permissions = take_bytes(&mut payload, permissions_length, "workspace read permissions")?;
        if !payload.is_empty() {
            return Err(InspectionError::Invalid(
                "workspace read record has trailing data".to_owned(),
            ));
        }
        let permissions = std::str::from_utf8(permissions)
            .map_err(|_| {
                InspectionError::Invalid("workspace read permissions are not UTF-8".to_owned())
            })?
            .to_owned();
        Ok(Self { observation, permissions })
    }
}

#[derive(Clone, Copy)]
struct ReadCursor {
    observation: Sha256Digest,
    request: Sha256Digest,
    offset: u64,
}

fn resolve_line_selection(
    retained: &mut RetainedInspection,
    first_line: u64,
    last_line: Option<u64>,
) -> Result<ResolvedSelection, InspectionError> {
    let source_bytes = retained.observation().source_bytes();
    if first_line == 1 && last_line.is_none() {
        return Ok(ResolvedSelection {
            start: 0,
            end: source_bytes,
        });
    }
    let mut start = (first_line == 1).then_some(0);
    let mut end = None;
    let mut line = 1_u64;
    let mut cursor = retained.cursor();
    while let Some(current) = cursor {
        let page = retained
            .read_page(current, SCAN_PAGE_BYTES)
            .map_err(InspectionError::Workspace)?
            .ok_or_else(|| {
                InspectionError::Invalid(
                    "retained workspace read ended before its source boundary".to_owned(),
                )
            })?;
        for (index, byte) in page.bytes().iter().enumerate() {
            if *byte != b'\n' {
                continue;
            }
            let position = page.range().0 + index as u64 + 1;
            if last_line == Some(line) {
                end = Some(position);
            }
            line = line.checked_add(1).ok_or_else(|| {
                InspectionError::Invalid("workspace read line counter overflow".to_owned())
            })?;
            if line == first_line {
                start = Some(position);
            }
        }
        if end.is_some() {
            break;
        }
        cursor = page.next();
    }
    let start = start.unwrap_or(source_bytes);
    let end = last_line.map_or(source_bytes, |_| end.unwrap_or(source_bytes)).max(start);
    Ok(ResolvedSelection { start, end })
}

fn position_at(
    retained: &mut RetainedInspection,
    selection_start: u64,
    target: u64,
    first_line: u64,
) -> Result<SourcePosition, InspectionError> {
    let mut position = SourcePosition { line: first_line, column_bytes: 0 };
    let mut offset = selection_start;
    while offset < target {
        let maximum = SCAN_PAGE_BYTES.min(target - offset);
        let cursor = retained.cursor_at(offset).map_err(InspectionError::Workspace)?;
        let page = retained
            .read_page(cursor, maximum)
            .map_err(InspectionError::Workspace)?
            .ok_or_else(|| {
                InspectionError::Invalid(
                    "retained workspace read ended before its continuation".to_owned(),
                )
            })?;
        position = advance_position(position, page.bytes())?;
        offset = page.range().1;
    }
    Ok(position)
}

fn read_utf8_page(
    retained: &mut RetainedInspection,
    offset: u64,
    maximum: u64,
) -> Result<(Vec<u8>, (u64, u64)), InspectionError> {
    let cursor = retained.cursor_at(offset).map_err(InspectionError::Workspace)?;
    let page = retained
        .read_page(cursor, maximum)
        .map_err(InspectionError::Workspace)?
        .ok_or_else(|| {
            InspectionError::Invalid("workspace read continuation is already complete".to_owned())
        })?;
    match std::str::from_utf8(page.bytes()) {
        Ok(_) => Ok((page.bytes().to_vec(), page.range())),
        Err(error) if error.error_len().is_none() && error.valid_up_to() > 0 => {
            let valid = error.valid_up_to() as u64;
            let page = retained
                .read_page(cursor, valid)
                .map_err(InspectionError::Workspace)?
                .ok_or_else(|| {
                    InspectionError::Invalid(
                        "workspace read UTF-8 boundary is unavailable".to_owned(),
                    )
                })?;
            std::str::from_utf8(page.bytes()).map_err(|error| {
                InspectionError::Invalid(format!(
                    "workspace read contains invalid UTF-8 at source byte {}",
                    page.range().0 + error.valid_up_to() as u64,
                ))
            })?;
            Ok((page.bytes().to_vec(), page.range()))
        }
        Err(error) => Err(InspectionError::Invalid(format!(
            "workspace read contains invalid UTF-8 at source byte {}",
            page.range().0 + error.valid_up_to() as u64,
        ))),
    }
}

fn advance_position(
    mut position: SourcePosition,
    bytes: &[u8],
) -> Result<SourcePosition, InspectionError> {
    for byte in bytes {
        if *byte == b'\n' {
            position.line = position.line.checked_add(1).ok_or_else(|| {
                InspectionError::Invalid("workspace read line counter overflow".to_owned())
            })?;
            position.column_bytes = 0;
        } else {
            position.column_bytes = position.column_bytes.checked_add(1).ok_or_else(|| {
                InspectionError::Invalid("workspace read column counter overflow".to_owned())
            })?;
        }
    }
    Ok(position)
}

fn last_rendered_line(first_line: u64, bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }
    let newlines = bytes.iter().filter(|byte| **byte == b'\n').count() as u64;
    Some(if bytes.last() == Some(&b'\n') {
        first_line.saturating_add(newlines.saturating_sub(1))
    } else {
        first_line.saturating_add(newlines)
    })
}

fn render_numbered(first_line: u64, bytes: &[u8]) -> Result<String, InspectionError> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        InspectionError::Invalid(format!(
            "workspace read contains invalid UTF-8 at page byte {}",
            error.valid_up_to(),
        ))
    })?;
    let mut output = String::new();
    let mut line = first_line;
    let mut remaining = text;
    while let Some(index) = remaining.find('\n') {
        let value = remaining[..index].strip_suffix('\r').unwrap_or(&remaining[..index]);
        if !output.is_empty() {
            output.push('\n');
        }
        write!(output, "{line}: {value}").map_err(|error| {
            InspectionError::Invalid(format!("render workspace read page: {error}"))
        })?;
        line = line.checked_add(1).ok_or_else(|| {
            InspectionError::Invalid("workspace read line counter overflow".to_owned())
        })?;
        remaining = &remaining[index + 1..];
    }
    if !remaining.is_empty() {
        let value = remaining.strip_suffix('\r').unwrap_or(remaining);
        if !output.is_empty() {
            output.push('\n');
        }
        write!(output, "{line}: {value}").map_err(|error| {
            InspectionError::Invalid(format!("render workspace read page: {error}"))
        })?;
    }
    Ok(output)
}

fn read_request_binding(
    path: &WorkspacePath,
    first_line: u64,
    last_line: Option<u64>,
) -> Sha256Digest {
    let mut bytes = b"peritus/workspace-read-request/v1\0".to_vec();
    bytes.extend_from_slice(&(path.as_str().len() as u64).to_be_bytes());
    bytes.extend_from_slice(path.as_str().as_bytes());
    bytes.extend_from_slice(&first_line.to_be_bytes());
    match last_line {
        Some(last) => {
            bytes.push(1);
            bytes.extend_from_slice(&last.to_be_bytes());
        }
        None => bytes.push(0),
    }
    peritus_codec::sha256(&bytes)
}

fn read_observation_handle(
    observation: Sha256Digest,
    request: Sha256Digest,
) -> Sha256Digest {
    let mut bytes = b"peritus/workspace-read-observation/v1\0".to_vec();
    bytes.extend_from_slice(observation.as_bytes());
    bytes.extend_from_slice(request.as_bytes());
    peritus_codec::sha256(&bytes)
}

fn encode_read_cursor(
    observation: Sha256Digest,
    request: Sha256Digest,
    offset: u64,
) -> String {
    let mut bytes = READ_CURSOR_MAGIC.to_vec();
    bytes.extend_from_slice(observation.as_bytes());
    bytes.extend_from_slice(request.as_bytes());
    bytes.extend_from_slice(&offset.to_be_bytes());
    encode_hex(&seal(bytes))
}

fn decode_read_cursor(value: &str) -> Result<ReadCursor, InspectionError> {
    let bytes = decode_hex(value)?;
    let mut payload = checked_record(&bytes, READ_CURSOR_MAGIC, "workspace read cursor")?;
    let observation = Sha256Digest::new(
        take_bytes(&mut payload, 32, "workspace read observation")?
            .try_into()
            .map_err(|_| {
                InspectionError::Invalid(
                    "workspace read observation handle is incomplete".to_owned(),
                )
            })?,
    );
    let request = Sha256Digest::new(
        take_bytes(&mut payload, 32, "workspace read request")?
            .try_into()
            .map_err(|_| {
                InspectionError::Invalid("workspace read request binding is incomplete".to_owned())
            })?,
    );
    let offset = u64::from_be_bytes(
        take_bytes(&mut payload, 8, "workspace read offset")?
            .try_into()
            .map_err(|_| {
                InspectionError::Invalid("workspace read offset is incomplete".to_owned())
            })?,
    );
    if !payload.is_empty() {
        return Err(InspectionError::Invalid(
            "workspace read cursor has trailing data".to_owned(),
        ));
    }
    Ok(ReadCursor { observation, request, offset })
}

pub(super) fn checked_record<'a>(
    bytes: &'a [u8],
    magic: &[u8; 8],
    kind: &str,
) -> Result<&'a [u8], InspectionError> {
    let end = bytes.len().checked_sub(32).ok_or_else(|| {
        InspectionError::Invalid(format!("{kind} is incomplete"))
    })?;
    let (payload, checksum) = bytes.split_at(end);
    if !payload.starts_with(magic) || peritus_codec::sha256(payload).as_bytes() != checksum {
        return Err(InspectionError::Invalid(format!(
            "{kind} version or checksum is invalid"
        )));
    }
    Ok(&payload[magic.len()..])
}

pub(super) fn take_length(
    bytes: &mut &[u8],
    kind: &str,
) -> Result<usize, InspectionError> {
    usize::try_from(u64::from_be_bytes(
        take_bytes(bytes, 8, kind)?
            .try_into()
            .map_err(|_| InspectionError::Invalid(format!("{kind} length is incomplete")))?,
    ))
    .map_err(|_| InspectionError::Invalid(format!("{kind} length is not representable")))
}

pub(super) fn take_bytes<'a>(
    bytes: &mut &'a [u8],
    length: usize,
    kind: &str,
) -> Result<&'a [u8], InspectionError> {
    let value = bytes
        .get(..length)
        .ok_or_else(|| InspectionError::Invalid(format!("{kind} is incomplete")))?;
    *bytes = &bytes[length..];
    Ok(value)
}

pub(super) fn seal(mut bytes: Vec<u8>) -> Vec<u8> {
    let checksum = peritus_codec::sha256(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    bytes
}

pub(super) fn digest_hex(digest: Sha256Digest) -> String {
    encode_hex(digest.as_bytes())
}

pub(super) fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn decode_hex(value: &str) -> Result<Vec<u8>, InspectionError> {
    if !value.len().is_multiple_of(2) {
        return Err(InspectionError::Invalid(
            "inspection continuation is not hexadecimal".to_owned(),
        ));
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

fn hex_digit(byte: u8) -> Result<u8, InspectionError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(InspectionError::Invalid(
            "inspection continuation is not hexadecimal".to_owned(),
        )),
    }
}

#[derive(Debug)]
pub(crate) enum InspectionError {
    Cancelled,
    Invalid(String),
    Storage(std::io::Error),
    Workspace(WorkspaceError),
}

impl fmt::Display for InspectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("workspace inspection was cancelled"),
            Self::Invalid(detail) => formatter.write_str(detail),
            Self::Storage(error) => write!(formatter, "workspace inspection storage: {error}"),
            Self::Workspace(error) => write!(formatter, "workspace inspection: {error}"),
        }
    }
}

impl std::error::Error for InspectionError {}

#[cfg(unix)]
pub(super) fn sync_storage_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
pub(super) fn sync_storage_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
