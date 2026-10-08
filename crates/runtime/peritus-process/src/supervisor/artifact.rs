//! Retained spool publication into the content-addressed artifact store.

use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, ArtifactStoreError, EncryptionMetadata, MediaType,
    ErrorCode as ArtifactErrorCode, RecoveryClass as ArtifactRecoveryClass, WriteRequest,
};
use peritus_types::EventId;

use crate::{
    ErrorCode, OutputArtifact, OutputStream, ProcessError, ProcessOperation, ProcessStore,
    RecoveryClass, TerminalResult, output::stream_spool_name,
};

use super::WaitAndPublishError;

pub(super) fn publish_spools(
    process_store: &ProcessStore,
    process_id: peritus_types::ProcessId,
    directory: &Path,
    artifacts: &ArtifactStore,
    creating_event: EventId,
) -> Result<TerminalResult, WaitAndPublishError> {
    let mut result =
        process_store.terminal_result(process_id).map_err(WaitAndPublishError::owner)?;
    if result.artifact_publication_complete() {
        return Ok(result);
    }
    let pending: Vec<_> = result
        .output()
        .streams()
        .iter()
        .filter(|stream| {
            stream.retained() > 0
                && !result.artifacts().iter().any(|artifact| artifact.stream() == stream.stream())
        })
        .copied()
        .collect();
    for (index, stream) in pending.iter().enumerate() {
        let artifact = publish_stream(directory, artifacts, creating_event, stream)
            .map_err(|error| WaitAndPublishError::publication(result.clone(), error))?;
        let complete = index + 1 == pending.len();
        result = match process_store.record_artifact_publication(process_id, artifact, complete) {
            Ok(result) => result,
            Err(error) => {
                return Err(publication_error(
                    process_store,
                    process_id,
                    result,
                    error,
                ));
            }
        };
    }
    if pending.is_empty() {
        result = match process_store.complete_artifact_publication(process_id) {
            Ok(result) => result,
            Err(error) => {
                return Err(publication_error(
                    process_store,
                    process_id,
                    result,
                    error,
                ));
            }
        };
    }
    Ok(result)
}

fn publication_error(
    process_store: &ProcessStore,
    process_id: peritus_types::ProcessId,
    predecessor: TerminalResult,
    source: ProcessError,
) -> WaitAndPublishError {
    // Manifest replacement can publish successfully and report a later sync/cleanup failure.
    // Re-observe the terminal while retaining that original typed failure as the retry cause.
    let terminal = process_store.terminal_result(process_id).unwrap_or(predecessor);
    WaitAndPublishError::publication(terminal, source)
}

fn publish_stream(
    directory: &Path,
    store: &ArtifactStore,
    creating_event: EventId,
    stream: &crate::StreamAccounting,
) -> Result<OutputArtifact, ProcessError> {
    let media_type = MediaType::new("application/octet-stream")
        .map_err(|error| {
            artifact_store_error("output artifact media type is invalid", error)
        })?;
    let files = StreamSpoolFiles::inspect(directory, stream.stream())?;
    let (digest, size) = hash_files(&files)?;
    if size != stream.retained() {
        return Err(artifact_integrity_error(
            "retained spool size differs from terminal accounting",
        ));
    }
    let request = WriteRequest::new(
        ArtifactDigest::from_sha256(digest),
        size,
        size,
        media_type,
        EncryptionMetadata::unencrypted(),
        creating_event,
    );
    let mut writer = store
        .begin_write(request)
        .map_err(|error| artifact_store_error("artifact writer cannot be created", error))?;
    let mut buffer = [0_u8; 16 * 1_024];
    files.for_each(|path| {
        let mut file = File::open(path)
            .map_err(|error| artifact_io_error("retained spool cannot be reopened", error))?;
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| artifact_io_error("retained spool cannot be read", error))?;
            if read == 0 {
                break;
            }
            writer.write_chunk(&buffer[..read]).map_err(|error| {
                artifact_store_error("retained spool cannot be written to artifact store", error)
            })?;
        }
        Ok(())
    })?;
    writer
        .finalize()
        .map_err(|error| artifact_store_error("output artifact cannot be finalized", error))?;
    Ok(OutputArtifact::new(stream.stream(), digest, size, 0, size, stream.completeness()))
}

enum StreamSpoolFiles {
    Legacy(PathBuf),
    Segmented { directory: PathBuf, base: &'static str, count: u64 },
}

impl StreamSpoolFiles {
    fn inspect(directory: &Path, stream: OutputStream) -> Result<Self, ProcessError> {
        let base = stream_spool_name(stream);
        let legacy = directory.join(base);
        let legacy_exists = legacy
            .try_exists()
            .map_err(|error| artifact_io_error("retained spool cannot be inspected", error))?;
        let prefix = format!("{base}.");
        let mut count = 0_u64;
        let mut greatest = None;
        let entries = fs::read_dir(directory)
            .map_err(|error| artifact_io_error("retained spool directory cannot be read", error))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| artifact_io_error("retained spool entry cannot be read", error))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(suffix) = name.strip_prefix(&prefix) else {
                continue;
            };
            if suffix.len() != 20 || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(artifact_integrity_error("retained spool segment name is invalid"));
            }
            let index = suffix
                .parse::<u64>()
                .map_err(|_| artifact_integrity_error("retained spool segment index is invalid"))?;
            count = count
                .checked_add(1)
                .ok_or_else(|| artifact_integrity_error("retained spool segment count overflowed"))?;
            greatest = Some(greatest.map_or(index, |current: u64| current.max(index)));
        }
        if legacy_exists && count > 0 {
            return Err(artifact_integrity_error(
                "legacy and segmented retained spools both exist",
            ));
        }
        if legacy_exists {
            let metadata = fs::symlink_metadata(&legacy)
                .map_err(|error| artifact_io_error("retained spool cannot be inspected", error))?;
            if !metadata.file_type().is_file() {
                return Err(artifact_integrity_error(
                    "legacy retained spool is not a regular file",
                ));
            }
            return Ok(Self::Legacy(legacy));
        }
        if count == 0 {
            return Err(artifact_io_error(
                "retained spool files are missing",
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "retained spool files are missing",
                ),
            ));
        }
        let expected_count = greatest
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| artifact_integrity_error("retained spool segment sequence overflowed"))?;
        if expected_count != count {
            return Err(artifact_integrity_error(
                "retained spool segment sequence is incomplete",
            ));
        }
        validate_segment_layout(directory, base, count)?;
        Ok(Self::Segmented { directory: directory.to_path_buf(), base, count })
    }

    fn for_each(
        &self,
        mut operation: impl FnMut(&Path) -> Result<(), ProcessError>,
    ) -> Result<(), ProcessError> {
        match self {
            Self::Legacy(path) => operation(path),
            Self::Segmented { directory, base, count } => {
                for index in 0..*count {
                    let path = directory.join(format!("{base}.{index:020}"));
                    operation(&path)?;
                }
                Ok(())
            }
        }
    }
}

fn validate_segment_layout(
    directory: &Path,
    base: &str,
    count: u64,
) -> Result<(), ProcessError> {
    let mut full_segment_bytes = None;
    for index in 0..count {
        let path = directory.join(format!("{base}.{index:020}"));
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| artifact_io_error("retained spool segment cannot be inspected", error))?;
        if !metadata.file_type().is_file() {
            return Err(artifact_integrity_error(
                "retained spool segment is not a regular file",
            ));
        }
        let length = metadata.len();
        // Rotation creates the next segment before its first write. A failed first write can
        // leave that final segment empty while all preceding bytes form the retained prefix.
        // The complete hash and exact retained-size check still authenticate that prefix.
        if length == 0 && index + 1 < count {
            return Err(artifact_integrity_error("retained spool interior segment is empty"));
        }
        if index + 1 < count {
            match full_segment_bytes {
                Some(expected) if length != expected => {
                    return Err(artifact_integrity_error(
                        "retained spool full segment lengths disagree",
                    ));
                }
                None => full_segment_bytes = Some(length),
                Some(_) => {}
            }
        } else if full_segment_bytes.is_some_and(|maximum| length > maximum) {
            return Err(artifact_integrity_error(
                "retained spool final segment exceeds the full segment length",
            ));
        }
    }
    Ok(())
}

fn hash_files(files: &StreamSpoolFiles) -> Result<(peritus_types::Sha256Digest, u64), ProcessError> {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 16 * 1_024];
    files.for_each(|path| {
        let mut file = File::open(path)
            .map_err(|error| artifact_io_error("retained spool cannot be opened", error))?;
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| artifact_io_error("retained spool cannot be hashed", error))?;
            if read == 0 {
                break;
            }
            size = size
                .checked_add(
                    u64::try_from(read).map_err(|_| {
                        artifact_integrity_error("artifact read length is unrepresentable")
                    })?,
                )
                .ok_or_else(|| artifact_integrity_error("artifact size accounting overflowed"))?;
            hasher.update(&buffer[..read]);
        }
        Ok(())
    })?;
    Ok((peritus_types::Sha256Digest::new(hasher.finalize().into()), size))
}

const fn artifact_integrity_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Artifact,
        ProcessOperation::PublishArtifact,
        RecoveryClass::Quarantine,
        detail,
    )
}

fn artifact_io_error(detail: &'static str, source: std::io::Error) -> ProcessError {
    ProcessError::with_source(
        ErrorCode::Artifact,
        ProcessOperation::PublishArtifact,
        RecoveryClass::RetryPublication,
        detail,
        source,
    )
}

fn artifact_store_error(detail: &'static str, source: ArtifactStoreError) -> ProcessError {
    let recovery = match source.code() {
        ArtifactErrorCode::QuotaExceeded
        | ArtifactErrorCode::StoragePressure
        | ArtifactErrorCode::CatalogBusy
        | ArtifactErrorCode::CatalogLocked
        | ArtifactErrorCode::CatalogWaitCancelled
        | ArtifactErrorCode::Io => RecoveryClass::RetryPublication,
        _ => match source.recovery_class() {
            ArtifactRecoveryClass::Retry | ArtifactRecoveryClass::RecoverStore => {
                RecoveryClass::RetryPublication
            }
            ArtifactRecoveryClass::CorrectRequest | ArtifactRecoveryClass::TerminalIntegrity => {
                RecoveryClass::Quarantine
            }
            _ => RecoveryClass::RetryPublication,
        },
    };
    ProcessError::with_source(
        ErrorCode::Artifact,
        ProcessOperation::PublishArtifact,
        recovery,
        detail,
        source,
    )
}
