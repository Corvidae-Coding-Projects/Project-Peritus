use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    time::Duration,
};

use peritus_app_protocol::{
    AppEventPayload, AppRequestPayload, AppResponsePayload, ArtifactCancellation, ArtifactChunk,
    ArtifactCompletion, ArtifactMetadata, ArtifactOpenRequest, ArtifactTransferState,
    CanonicalMediaType, TransferId, WellKnownProtocolFeature,
};
use peritus_types::{ArtifactId, SessionId, Sha256Digest};
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use crate::{
    args::{ArtifactCancelArgs, ArtifactGetArgs, ArtifactPutArgs},
    client::Client,
    error::CliError,
    id::{generated_id, hex},
    operation::response_error,
    output::Output,
};

pub async fn cancel(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    arguments: ArtifactCancelArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client =
        Client::connect(endpoint, session, timeout, &[WellKnownProtocolFeature::ArtifactTransfer])
            .await?;
    let transfer = TransferId::new(arguments.transfer)
        .map_err(|_| CliError::usage("invalid --transfer identifier"))?;
    let artifact = ArtifactId::new(arguments.artifact)
        .map_err(|_| CliError::usage("invalid --artifact identifier"))?;
    let identity = Client::new_request_identity()?;
    let cancellation = ArtifactCancellation::new(transfer, artifact, identity.correlation_id);
    let response =
        client.request(identity, AppRequestPayload::CancelArtifact(cancellation)).await?;
    expect_ack(response.payload())?;
    output.success(
        "artifact-cancelled",
        serde_json::json!({
            "transfer_id": hex(transfer.as_bytes()),
            "artifact_id": hex(artifact.as_bytes()),
        }),
        &format!("artifact transfer {} cancelled", hex(transfer.as_bytes())),
    )
}

pub async fn get(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    arguments: ArtifactGetArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client =
        Client::connect(endpoint, session, timeout, &[WellKnownProtocolFeature::ArtifactTransfer])
            .await?;
    let transfer_id = TransferId::new(generated_id(b"artifact-download"))
        .map_err(|_| CliError::runtime("create transfer identity", "generated zero identifier"))?;
    let artifact_id = ArtifactId::new(arguments.artifact)
        .map_err(|_| CliError::usage("invalid --artifact identifier"))?;
    let identity = Client::new_request_identity()?;
    let response = client
        .request(
            identity,
            AppRequestPayload::OpenArtifact(ArtifactOpenRequest::new(transfer_id, artifact_id)),
        )
        .await?;
    let AppResponsePayload::ArtifactOpened(metadata) = response.payload() else {
        return response_error(response.payload(), "artifact metadata");
    };
    if metadata.transfer_id() != transfer_id || metadata.artifact_id() != artifact_id {
        return Err(CliError::protocol(
            "validate artifact metadata",
            "daemon opened a different transfer or artifact",
        ));
    }

    let temporary = temporary_path(&arguments.output);
    let download = receive_download(&mut client, metadata.clone(), &temporary).await;
    if let Err(error) = download {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error);
    }
    publish_download(&temporary, &arguments.output, arguments.force).await?;
    output.success(
        "artifact-downloaded",
        serde_json::json!({
            "artifact_id": hex(artifact_id.as_bytes()),
            "transfer_id": hex(transfer_id.as_bytes()),
            "bytes": metadata.byte_size(),
            "sha256": hex(metadata.digest().as_bytes()),
            "media_type": metadata.media_type().as_str(),
            "output": arguments.output.to_string_lossy(),
        }),
        &format!(
            "artifact {} downloaded to {} ({} bytes, sha256={})",
            hex(artifact_id.as_bytes()),
            arguments.output.display(),
            metadata.byte_size(),
            hex(metadata.digest().as_bytes()),
        ),
    )
}

async fn receive_download(
    client: &mut Client,
    metadata: ArtifactMetadata,
    temporary: &Path,
) -> Result<(), CliError> {
    let mut file =
        tokio::fs::OpenOptions::new().write(true).create_new(true).open(temporary).await.map_err(
            |error| {
                CliError::local_io(
                    "create temporary artifact output",
                    Some(temporary.to_path_buf()),
                    error,
                )
            },
        )?;
    let mut transfer =
        ArtifactTransferState::new(metadata.clone(), client.limits().max_artifact_chunk_bytes())
            .map_err(|error| {
                CliError::protocol("initialize artifact download", error.to_string())
            })?;
    let mut hasher = Sha256::new();
    loop {
        let event = client.read_event().await?;
        if client.reply_heartbeat(&event).await? {
            continue;
        }
        match event.payload() {
            AppEventPayload::ArtifactMetadata(observed)
                if observed.transfer_id() == metadata.transfer_id() =>
            {
                if observed != &metadata {
                    return Err(CliError::protocol(
                        "stream artifact download",
                        "stream metadata differs from opened metadata",
                    ));
                }
            }
            AppEventPayload::ArtifactChunk(chunk)
                if chunk.transfer_id() == metadata.transfer_id() =>
            {
                transfer.accept_chunk(chunk).map_err(|error| {
                    CliError::protocol("stream artifact download", error.to_string())
                })?;
                file.write_all(chunk.bytes()).await.map_err(|error| {
                    CliError::local_io(
                        "write temporary artifact output",
                        Some(temporary.to_path_buf()),
                        error,
                    )
                })?;
                hasher.update(chunk.bytes());
            }
            AppEventPayload::ArtifactComplete(completion)
                if completion.transfer_id() == metadata.transfer_id() =>
            {
                if completion.artifact_id() != metadata.artifact_id()
                    || completion.byte_size() != metadata.byte_size()
                    || completion.digest() != metadata.digest()
                {
                    return Err(CliError::protocol(
                        "complete artifact download",
                        "completion metadata differs from opened metadata",
                    ));
                }
                let digest: [u8; 32] = hasher.finalize().into();
                transfer.complete(Sha256Digest::new(digest)).map_err(|error| {
                    CliError::protocol("complete artifact download", error.to_string())
                })?;
                file.flush().await.map_err(|error| {
                    CliError::local_io(
                        "flush temporary artifact output",
                        Some(temporary.to_path_buf()),
                        error,
                    )
                })?;
                file.sync_all().await.map_err(|error| {
                    CliError::local_io(
                        "sync temporary artifact output",
                        Some(temporary.to_path_buf()),
                        error,
                    )
                })?;
                return Ok(());
            }
            _ => {}
        }
    }
}

pub async fn put(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    arguments: ArtifactPutArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client =
        Client::connect(endpoint, session, timeout, &[WellKnownProtocolFeature::ArtifactTransfer])
            .await?;
    let chunk_size = usize::try_from(arguments.chunk_size)
        .map_err(|_| CliError::usage("--chunk-size cannot be represented on this platform"))?;
    if chunk_size > client.limits().max_artifact_chunk_bytes() {
        return Err(CliError::usage(format!(
            "--chunk-size exceeds negotiated maximum {}",
            client.limits().max_artifact_chunk_bytes(),
        )));
    }
    let (byte_size, digest) = digest_file(&arguments.input).await?;
    let artifact_id = ArtifactId::new(arguments.artifact)
        .map_err(|_| CliError::usage("invalid --artifact identifier"))?;
    let transfer_id = TransferId::new(generated_id(b"artifact-upload"))
        .map_err(|_| CliError::runtime("create transfer identity", "generated zero identifier"))?;
    let media_type =
        CanonicalMediaType::new(arguments.media_type, client.limits().codec().max_string_bytes)
            .map_err(|error| CliError::usage(error.to_string()))?;
    let metadata = ArtifactMetadata::new(
        transfer_id,
        artifact_id,
        byte_size,
        media_type,
        digest,
        arguments.chunk_size,
        client.limits().max_artifact_chunk_bytes(),
    )
    .map_err(|error| CliError::usage(error.to_string()))?;
    let identity = Client::new_request_identity()?;
    let response =
        client.request(identity, AppRequestPayload::BeginArtifactUpload(metadata.clone())).await?;
    expect_ack(response.payload())?;

    let (offset, ordinal) =
        upload_chunks(&mut client, &arguments.input, transfer_id, artifact_id, chunk_size).await?;
    if offset != byte_size {
        return Err(CliError::local_io(
            "read artifact input",
            Some(arguments.input),
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "file size changed between digest and upload",
            ),
        ));
    }
    let completion = ArtifactCompletion::new(transfer_id, artifact_id, byte_size, digest);
    let identity = Client::new_request_identity()?;
    let response =
        client.request(identity, AppRequestPayload::CompleteArtifactUpload(completion)).await?;
    expect_ack(response.payload())?;
    output.success(
        "artifact-uploaded",
        serde_json::json!({
            "artifact_id": hex(artifact_id.as_bytes()),
            "transfer_id": hex(transfer_id.as_bytes()),
            "bytes": byte_size,
            "sha256": hex(digest.as_bytes()),
            "chunks": ordinal,
        }),
        &format!(
            "artifact {} uploaded ({} bytes, {} chunks, sha256={})",
            hex(artifact_id.as_bytes()),
            byte_size,
            ordinal,
            hex(digest.as_bytes()),
        ),
    )
}

async fn upload_chunks(
    client: &mut Client,
    input: &Path,
    transfer_id: TransferId,
    artifact_id: ArtifactId,
    chunk_size: usize,
) -> Result<(u64, u64), CliError> {
    let mut file = tokio::fs::File::open(input).await.map_err(|error| {
        CliError::local_io("open artifact input", Some(input.to_path_buf()), error)
    })?;
    let mut buffer = vec![0_u8; chunk_size];
    let mut ordinal = 0_u64;
    let mut offset = 0_u64;
    loop {
        let read = file.read(&mut buffer).await.map_err(|error| {
            CliError::local_io("read artifact input", Some(input.to_path_buf()), error)
        })?;
        if read == 0 {
            return Ok((offset, ordinal));
        }
        let chunk = ArtifactChunk::new(
            transfer_id,
            artifact_id,
            ordinal,
            offset,
            buffer[..read].to_vec(),
            client.limits().max_artifact_chunk_bytes(),
        )
        .map_err(|error| CliError::protocol("construct artifact chunk", error.to_string()))?;
        let identity = Client::new_request_identity()?;
        let response =
            client.request(identity, AppRequestPayload::UploadArtifactChunk(chunk)).await?;
        expect_ack(response.payload())?;
        let read_u64 = u64::try_from(read)
            .map_err(|_| CliError::runtime("count artifact bytes", "chunk length overflow"))?;
        offset = offset
            .checked_add(read_u64)
            .ok_or_else(|| CliError::runtime("count artifact bytes", "byte offset overflow"))?;
        ordinal = ordinal
            .checked_add(1)
            .ok_or_else(|| CliError::runtime("count artifact chunks", "chunk ordinal overflow"))?;
    }
}

async fn digest_file(path: &Path) -> Result<(u64, Sha256Digest), CliError> {
    let mut file = tokio::fs::File::open(path).await.map_err(|error| {
        CliError::local_io("open artifact input", Some(path.to_path_buf()), error)
    })?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await.map_err(|error| {
            CliError::local_io("read artifact input", Some(path.to_path_buf()), error)
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total =
            total
                .checked_add(u64::try_from(read).map_err(|_| {
                    CliError::runtime("count artifact bytes", "read length overflow")
                })?)
                .ok_or_else(|| CliError::runtime("count artifact bytes", "file size overflow"))?;
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Ok((total, Sha256Digest::new(digest)))
}

async fn publish_download(temporary: &Path, output: &Path, force: bool) -> Result<(), CliError> {
    if !force {
        match tokio::fs::symlink_metadata(output).await {
            Ok(_) => {
                return Err(CliError::local_io(
                    "publish artifact output",
                    Some(output.to_path_buf()),
                    std::io::Error::new(std::io::ErrorKind::AlreadyExists, "output already exists"),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(CliError::local_io(
                    "inspect artifact output",
                    Some(output.to_path_buf()),
                    error,
                ));
            }
        }
    } else if let Err(error) = tokio::fs::remove_file(output).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(CliError::local_io(
            "replace artifact output",
            Some(output.to_path_buf()),
            error,
        ));
    }
    tokio::fs::rename(temporary, output).await.map_err(|error| {
        CliError::local_io("publish artifact output", Some(output.to_path_buf()), error)
    })
}

fn temporary_path(output: &Path) -> PathBuf {
    let suffix = hex(&generated_id(b"artifact-temporary"));
    let mut name = output.file_name().map_or_else(|| "artifact".into(), std::ffi::OsString::from);
    name.push(format!(".peritus-{suffix}.part"));
    output.with_file_name(name)
}

fn expect_ack(payload: &AppResponsePayload) -> Result<(), CliError> {
    match payload {
        AppResponsePayload::Acknowledged(_) => Ok(()),
        _ => response_error(payload, "operation acknowledgement"),
    }
}
