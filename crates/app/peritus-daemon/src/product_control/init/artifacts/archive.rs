//! Owned artifact capture and exact immutable reads for initialization review.

use super::{
    CHUNK_BYTES, Content, Manifest, Object, Source, app_error, mode_code, observed_mode,
    unavailable,
};
use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, InitContentReference, InitDiscoveryRequest,
};
use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_patch::WorkspacePath;
use peritus_types::{EventId, Sha256Digest};
use peritus_workspace::FolderInspection;
use std::{cell::RefCell, fs, path::Path};

pub(super) struct Archive {
    pub(super) config: StoreConfig,
    pub(super) store: ArtifactStore,
    _owner: fs::File,
}
impl Archive {
    pub(super) fn open(path: &Path) -> Result<Self, AppProtocolError> {
        fs::create_dir_all(path).map_err(unavailable)?;
        let owner = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.join("owner.lock"))
            .map_err(unavailable)?;
        owner.lock().map_err(unavailable)?;
        let config = config(path)?;
        let store = ArtifactStore::open(config.clone()).map_err(unavailable)?;
        Ok(Self { config, store, _owner: owner })
    }
    pub(super) fn object(&self, bytes: &[u8]) -> Result<Object, AppProtocolError> {
        let digest = peritus_codec::sha256(bytes);
        let mut id = [0; 16];
        id.copy_from_slice(&digest.as_bytes()[..16]);
        id[0] |= 1;
        let request = WriteRequest::new(
            ArtifactDigest::from_sha256(digest),
            bytes.len() as u64,
            (bytes.len() as u64).max(1),
            MediaType::new("application/octet-stream").map_err(unavailable)?,
            EncryptionMetadata::unencrypted(),
            EventId::new(id).map_err(unavailable)?,
        );
        let mut writer = self.store.begin_owned_write(request).map_err(unavailable)?;
        writer.write_chunk(bytes).map_err(unavailable)?;
        self.store.complete_write(writer).map_err(unavailable)?;
        Ok(Object { digest: digest.into_bytes(), bytes: bytes.len() as u64 })
    }
    pub(super) fn content(&self, bytes: &[u8]) -> Result<Content, AppProtocolError> {
        let chunks =
            bytes.chunks(CHUNK_BYTES).map(|chunk| self.object(chunk)).collect::<Result<_, _>>()?;
        Ok(Content {
            digest: peritus_codec::sha256(bytes).into_bytes(),
            bytes: bytes.len() as u64,
            chunks,
        })
    }
    pub(super) fn source(
        &self,
        reader: &FolderInspection,
        root: &Path,
        path: &str,
        kind: u8,
        explicit: bool,
    ) -> Result<Source, AppProtocolError> {
        let mut result = Source {
            path: path.to_owned(),
            kind,
            explicit,
            content: None,
            diagnostic: None,
            mode: 1,
        };
        let metadata = match fs::symlink_metadata(root.join(path)) {
            Ok(metadata) if metadata.file_type().is_file() => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if explicit {
                    result.diagnostic =
                        Some("explicitly selected source does not exist".to_owned());
                }
                return Ok(result);
            }
            _ => {
                result.diagnostic =
                    Some("source is unavailable or is not a no-follow regular file".to_owned());
                return Ok(result);
            }
        };
        result.mode = mode_code(observed_mode(&metadata));
        let path =
            WorkspacePath::new(path).map_err(|_| app_error(AppErrorCode::InvalidIdentifier))?;
        let failure = RefCell::new(None);
        let mut chunks = Vec::new();
        let scanned = reader.scan_file_chunks(
            &path,
            || failure.borrow().is_some(),
            |_, bytes| match self.object(bytes) {
                Ok(object) => chunks.push(object),
                Err(error) => {
                    *failure.borrow_mut() = Some(error);
                }
            },
        );
        if let Some(error) = failure.into_inner() {
            return Err(error);
        }
        match scanned {
            Ok(Some((bytes, digest))) => {
                result.content = Some(Content { digest: digest.into_bytes(), bytes, chunks });
            }
            _ => {
                result.diagnostic = Some(
                    "source changed during capture or could not be inspected safely".to_owned(),
                );
            }
        }
        Ok(result)
    }
}

pub(super) fn config(root: &Path) -> Result<StoreConfig, AppProtocolError> {
    StoreConfig::new_with_quota_policy(root.join("objects"), i64::MAX as u64, None)
        .map_err(unavailable)
}
pub(super) fn load_manifest(
    config: &StoreConfig,
    request: InitDiscoveryRequest,
    reference: InitContentReference,
) -> Result<Manifest, AppProtocolError> {
    let bytes = read_object(
        config,
        &Object { digest: reference.digest().into_bytes(), bytes: reference.bytes() },
    )?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(unavailable)?;
    if manifest.version != 1
        || manifest.conversation != request.query().conversation().into_bytes()
        || manifest.workspace != request.query().workspace().into_bytes()
        || manifest.revision != request.revision()
    {
        return Err(app_error(AppErrorCode::StaleRevision));
    }
    Ok(manifest)
}
pub(super) fn read_object(
    config: &StoreConfig,
    object: &Object,
) -> Result<Vec<u8>, AppProtocolError> {
    let bytes = ArtifactStore::read_existing(
        config,
        ArtifactDigest::from_sha256(Sha256Digest::new(object.digest)),
        object.bytes.max(1),
    )
    .map_err(unavailable)?;
    if bytes.len() as u64 != object.bytes {
        return Err(app_error(AppErrorCode::MalformedFrame));
    }
    Ok(bytes)
}
pub(super) fn read_content(
    config: &StoreConfig,
    content: &Content,
) -> Result<Vec<u8>, AppProtocolError> {
    let bytes = content_range(config, content, 0, content.bytes)?;
    if bytes.len() as u64 != content.bytes
        || peritus_codec::sha256(&bytes).into_bytes() != content.digest
    {
        return Err(app_error(AppErrorCode::MalformedFrame));
    }
    Ok(bytes)
}
pub(super) fn content_range(
    config: &StoreConfig,
    content: &Content,
    offset: u64,
    maximum: u64,
) -> Result<Vec<u8>, AppProtocolError> {
    let end = offset.saturating_add(maximum).min(content.bytes);
    let mut bytes = Vec::new();
    let mut cursor = 0_u64;
    for object in &content.chunks {
        let next = cursor
            .checked_add(object.bytes)
            .ok_or_else(|| app_error(AppErrorCode::MalformedFrame))?;
        if next > offset && cursor < end {
            let chunk = read_object(config, object)?;
            let start = usize::try_from(offset.saturating_sub(cursor)).map_err(unavailable)?;
            let stop = usize::try_from(end.min(next) - cursor).map_err(unavailable)?;
            bytes.try_reserve(stop - start).map_err(unavailable)?;
            bytes.extend_from_slice(&chunk[start..stop]);
        }
        cursor = next;
    }
    if cursor != content.bytes || offset > cursor || bytes.len() as u64 != end - offset {
        return Err(app_error(AppErrorCode::MalformedFrame));
    }
    Ok(bytes)
}
