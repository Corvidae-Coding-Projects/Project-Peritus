//! Retained spool publication into the content-addressed artifact store.

use std::{fs::File, io::Read, path::Path};

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, WriteRequest,
};
use peritus_types::EventId;

use crate::{
    ErrorCode, OutputArtifact, OutputStream, ProcessError, ProcessOperation, ProcessStore,
    RecoveryClass, TerminalResult,
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
        result = process_store
            .record_artifact_publication(process_id, artifact, complete)
            .map_err(|error| WaitAndPublishError::publication(result.clone(), error))?;
    }
    if pending.is_empty() {
        result = process_store
            .complete_artifact_publication(process_id)
            .map_err(|error| WaitAndPublishError::publication(result.clone(), error))?;
    }
    Ok(result)
}

fn publish_stream(
    directory: &Path,
    store: &ArtifactStore,
    creating_event: EventId,
    stream: &crate::StreamAccounting,
) -> Result<OutputArtifact, ProcessError> {
    let media_type = MediaType::new("application/octet-stream")
        .map_err(|_| artifact_error("output artifact media type is invalid"))?;
    let path = directory.join(match stream.stream() {
        OutputStream::Stdout => "stdout.spool",
        OutputStream::Stderr => "stderr.spool",
        OutputStream::Terminal => "terminal.spool",
    });
    let (digest, size) = hash_file(&path)?;
    if size != stream.retained() {
        return Err(artifact_error("retained spool size differs from terminal accounting"));
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
        .map_err(|_| artifact_error("artifact writer cannot be created"))?;
    let mut file =
        File::open(&path).map_err(|_| artifact_error("retained spool cannot be reopened"))?;
    let mut buffer = [0_u8; 16 * 1_024];
    loop {
        let read =
            file.read(&mut buffer).map_err(|_| artifact_error("retained spool cannot be read"))?;
        if read == 0 {
            break;
        }
        writer
            .write_chunk(&buffer[..read])
            .map_err(|_| artifact_error("retained spool cannot be written to artifact store"))?;
    }
    writer.finalize().map_err(|_| artifact_error("output artifact cannot be finalized"))?;
    Ok(OutputArtifact::new(stream.stream(), digest, size, 0, size, stream.completeness()))
}

fn hash_file(path: &Path) -> Result<(peritus_types::Sha256Digest, u64), ProcessError> {
    use sha2::Digest as _;
    let mut file =
        File::open(path).map_err(|_| artifact_error("retained spool cannot be opened"))?;
    let mut hasher = sha2::Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 16 * 1_024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| artifact_error("retained spool cannot be hashed"))?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(u64::try_from(read).unwrap_or(u64::MAX))
            .ok_or_else(|| artifact_error("artifact size accounting overflowed"))?;
        hasher.update(&buffer[..read]);
    }
    Ok((peritus_types::Sha256Digest::new(hasher.finalize().into()), size))
}

const fn artifact_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Artifact,
        ProcessOperation::PublishArtifact,
        RecoveryClass::RetryPublication,
        detail,
    )
}
