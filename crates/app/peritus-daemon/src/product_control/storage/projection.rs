//! Disposable replay checkpoints bound to journal-owned historical projection evidence.

use super::Error;
use peritus_codec::sha256;
use peritus_journal::StoreId;
use peritus_product_runner::control::{ControlError, ConversationId};
use peritus_types::EventId;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const CURRENT_SCHEMA: u16 = 2;
const DIRECTORY: &str = "conversation-projections-v1";
const CURRENT_FILE: &str = "current.projection";
// Only the fixed metadata frame is bounded. The digest-checked body has no logical cache cap.
const MAX_HEADER_BYTES: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EventBinding {
    pub(super) sequence: u64,
    pub(super) id: [u8; 16],
    pub(super) hash: [u8; 32],
    pub(super) global_position: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RootBinding {
    pub(super) revision: u64,
    pub(super) digest: [u8; 32],
    pub(super) producing_position: u64,
}

pub(super) struct CurrentProjection {
    pub(super) head: EventBinding,
    pub(super) root: RootBinding,
    pub(super) replay: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentHeader {
    schema: u16,
    store: [u8; 16],
    conversation: [u8; 16],
    head: EventBinding,
    root: RootBinding,
    replay_bytes: u64,
    replay_digest: [u8; 32],
    header_digest: [u8; 32],
}

pub(super) struct ConversationProjections {
    root: PathBuf,
    store: [u8; 16],
}

impl ConversationProjections {
    pub(super) fn open(root: &Path, store: StoreId) -> Result<Self, Error> {
        let projection_root = root.join(DIRECTORY);
        ensure_child_directory(root, &projection_root)?;
        Ok(Self { root: projection_root, store: *store.as_bytes() })
    }

    pub(super) fn read_current(
        &self,
        conversation: ConversationId,
    ) -> Result<Option<CurrentProjection>, Error> {
        let path = self.conversation_path(conversation).join(CURRENT_FILE);
        let Some((mut file, file_bytes)) = open_projection_file(&path)? else {
            return Ok(None);
        };
        let Some(header_bytes) = read_header(&mut file)? else {
            return Ok(None);
        };
        let Ok(header) = serde_json::from_slice::<CurrentHeader>(&header_bytes) else {
            return Ok(None);
        };
        if header.schema != CURRENT_SCHEMA
            || header.store != self.store
            || header.conversation != *conversation.as_bytes()
            || !current_header_identity_valid(&header)
            || header.header_digest != current_header_digest(&header)
            || serde_json::to_vec(&header).ok().as_deref() != Some(header_bytes.as_slice())
        {
            return Ok(None);
        }
        let Some(replay) = read_body(
            file,
            file_bytes,
            header_bytes.len(),
            header.replay_bytes,
            header.replay_digest,
        )? else {
            return Ok(None);
        };
        Ok(Some(CurrentProjection { head: header.head, root: header.root, replay }))
    }

    pub(super) fn write_current(
        &self,
        conversation: ConversationId,
        projection: &CurrentProjection,
    ) -> Result<(), Error> {
        let directory = self.prepare_conversation_directory(conversation)?;
        let replay_bytes = u64::try_from(projection.replay.len())
            .map_err(|_| ControlError::Capacity)?;
        let mut header = CurrentHeader {
            schema: CURRENT_SCHEMA,
            store: self.store,
            conversation: *conversation.as_bytes(),
            head: projection.head,
            root: projection.root,
            replay_bytes,
            replay_digest: sha256(&projection.replay).into_bytes(),
            header_digest: [0; 32],
        };
        header.header_digest = current_header_digest(&header);
        write_file(&directory.join(CURRENT_FILE), &header, &projection.replay)
    }

    fn prepare_conversation_directory(
        &self,
        conversation: ConversationId,
    ) -> Result<PathBuf, Error> {
        let directory = self.conversation_path(conversation);
        ensure_child_directory(&self.root, &directory)?;
        Ok(directory)
    }

    fn conversation_path(&self, conversation: ConversationId) -> PathBuf {
        self.root.join(hex(conversation.as_bytes()))
    }
}

fn open_projection_file(path: &Path) -> Result<Option<(File, u64)>, Error> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(Error::Corrupt("unsafe conversation projection file"));
    }
    Ok(Some((File::open(path)?, metadata.len())))
}

fn read_header(file: &mut File) -> Result<Option<Vec<u8>>, Error> {
    let mut header = Vec::with_capacity(1024);
    while header.len() <= MAX_HEADER_BYTES {
        let mut byte = [0_u8; 1];
        match file.read(&mut byte) {
            Ok(0) => return Ok(None),
            Ok(1) if byte[0] == b'\n' => {
                return Ok((!header.is_empty() && header.len() <= MAX_HEADER_BYTES)
                    .then_some(header));
            }
            Ok(1) => header.push(byte[0]),
            Ok(_) => return Err(Error::Corrupt("invalid conversation projection header read")),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(None)
}

fn read_body(
    mut file: File,
    file_bytes: u64,
    header_bytes: usize,
    body_bytes: u64,
    expected_digest: [u8; 32],
) -> Result<Option<Vec<u8>>, Error> {
    let Some(expected_file_bytes) = u64::try_from(header_bytes)
        .ok()
        .and_then(|bytes| bytes.checked_add(1))
        .and_then(|bytes| bytes.checked_add(body_bytes))
    else {
        return Ok(None);
    };
    if expected_file_bytes != file_bytes {
        return Ok(None);
    }
    let body_length = usize::try_from(body_bytes).map_err(|_| ControlError::Capacity)?;
    let mut body = Vec::new();
    body.try_reserve_exact(body_length).map_err(|_| ControlError::Capacity)?;
    body.resize(body_length, 0);
    if let Err(error) = file.read_exact(&mut body) {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            return Ok(None);
        }
        return Err(error.into());
    }
    if sha256(&body).into_bytes() != expected_digest {
        return Ok(None);
    }
    Ok(Some(body))
}

fn write_file<T: Serialize>(path: &Path, header: &T, body: &[u8]) -> Result<(), Error> {
    let header = serde_json::to_vec(header)
        .map_err(|_| Error::Corrupt("cannot encode conversation projection header"))?;
    if header.is_empty() || header.len() > MAX_HEADER_BYTES {
        return Err(Error::Corrupt("conversation projection header exceeds its physical frame"));
    }
    let parent = path.parent().ok_or(Error::Corrupt("conversation projection has no directory"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&header)?;
    temporary.write_all(b"\n")?;
    temporary.write_all(body)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    sync_directory(parent)?;
    Ok(())
}

fn ensure_child_directory(parent: &Path, path: &Path) -> Result<(), Error> {
    let created = match fs::create_dir(path) {
        Ok(()) => true,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => false,
        Err(error) => return Err(error.into()),
    };
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(Error::Corrupt("unsafe conversation projection directory"));
    }
    if created {
        sync_directory(parent)?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(not(windows))]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(0x0200_0000)
            .open(path)?
            .sync_all()
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(value, "{byte:02x}");
    }
    value
}

fn current_header_digest(header: &CurrentHeader) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(256);
    bytes.extend_from_slice(b"peritus/product-control/current-projection-header/v2");
    bytes.extend_from_slice(&header.schema.to_be_bytes());
    bytes.extend_from_slice(&header.store);
    bytes.extend_from_slice(&header.conversation);
    append_event_binding(&mut bytes, header.head);
    append_root_binding(&mut bytes, header.root);
    bytes.extend_from_slice(&header.replay_bytes.to_be_bytes());
    bytes.extend_from_slice(&header.replay_digest);
    sha256(&bytes).into_bytes()
}

fn current_header_identity_valid(header: &CurrentHeader) -> bool {
    EventId::new(header.head.id).is_ok()
        && header.head.sequence != 0
        && header.head.sequence <= header.head.global_position
        && header.head.global_position <= header.root.producing_position
        && header.root.revision == header.head.sequence
        && i64::try_from(header.head.sequence).is_ok()
        && i64::try_from(header.head.global_position).is_ok()
        && i64::try_from(header.root.revision).is_ok()
        && i64::try_from(header.root.producing_position).is_ok()
}

fn append_event_binding(bytes: &mut Vec<u8>, binding: EventBinding) {
    bytes.extend_from_slice(&binding.sequence.to_be_bytes());
    bytes.extend_from_slice(&binding.id);
    bytes.extend_from_slice(&binding.hash);
    bytes.extend_from_slice(&binding.global_position.to_be_bytes());
}

fn append_root_binding(bytes: &mut Vec<u8>, binding: RootBinding) {
    bytes.extend_from_slice(&binding.revision.to_be_bytes());
    bytes.extend_from_slice(&binding.digest);
    bytes.extend_from_slice(&binding.producing_position.to_be_bytes());
}
