use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

use peritus_app_protocol::{
    AppErrorCode, AppEventPayload, AppRequestPayload, AppResponsePayload, ArtifactCancellation,
    ArtifactChunk, ArtifactCompletion, ArtifactMetadata, ArtifactOpenRequest,
    ArtifactTransferState, CanonicalMediaType, CorrelationId, RequestId, TransferId,
    WellKnownProtocolFeature,
};
use peritus_types::{ArtifactId, SessionId, Sha256Digest};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _, AsyncWriteExt as _, SeekFrom};

use crate::{
    args::{ArtifactCancelArgs, ArtifactGetArgs, ArtifactPutArgs},
    client::{Client, RequestIdentity},
    error::{CliError, ExitCategory},
    id::{hex, parse_hex_digest, parse_hex_id},
    operation::response_error,
    output::Output,
    recovery,
};

const HASH_BUFFER_BYTES: usize = 64 * 1024;

pub async fn cancel(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
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
    let response = client
        .request(identity, AppRequestPayload::CancelArtifact(cancellation))
        .await?;
    expect_ack(response.payload(), identity, "artifact cancellation")?;
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
    timeout: Option<Duration>,
    arguments: ArtifactGetArgs,
    output: &Output,
) -> Result<(), CliError> {
    ensure_distinct_paths(
        arguments.receipt.as_deref(),
        &arguments.output,
        "--receipt and --output must name different files",
    )
    .await?;
    let _receipt_lease = recovery::lease(arguments.receipt.as_deref()).await?;
    let artifact_id = ArtifactId::new(arguments.artifact)
        .map_err(|_| CliError::usage("invalid --artifact identifier"))?;
    let invocation = download_invocation_fingerprint(endpoint, &arguments);
    let retained = match arguments.receipt.as_deref() {
        Some(path) => recovery::load::<DownloadReceipt>(path).await?,
        None => None,
    };
    if let Some(receipt) = &retained {
        validate_download_receipt(receipt, &invocation, endpoint, &arguments, session)?;
        ensure_distinct_paths(
            arguments.receipt.as_deref(),
            &temporary_path(&arguments.output, stored_transfer(&receipt.transfer_id)?),
            "--receipt conflicts with the retained artifact temporary",
        )
        .await?;
        if matches!(
            receipt.phase,
            DownloadPhase::Verified | DownloadPhase::Published | DownloadPhase::Reported
        ) {
            let prior_output = match receipt.phase {
                DownloadPhase::Published => Some("possible"),
                DownloadPhase::Reported => Some("confirmed"),
                DownloadPhase::OpenPrepared
                | DownloadPhase::Receiving
                | DownloadPhase::Verified => None,
            };
            let mut receipt = receipt.clone();
            return finish_download(
                &arguments,
                &mut receipt,
                arguments.receipt.as_deref(),
                output,
                None,
                prior_output,
            )
            .await;
        }
    }

    let requested_session = retained
        .as_ref()
        .map(|receipt| stored_session(&receipt.session_id))
        .transpose()?
        .or(session);
    let mut client = connect_artifact(endpoint, requested_session, timeout).await?;
    let mut receipt = match retained {
        Some(receipt) => receipt,
        None => {
            let transfer_id = fresh_transfer_id()?;
            let seed = Client::new_request_identity()?;
            let session_id = client.context().session_id();
            let temporary = temporary_path(&arguments.output, transfer_id);
            ensure_distinct_paths(
                arguments.receipt.as_deref(),
                &temporary,
                "--receipt conflicts with the artifact temporary",
            )
            .await?;
            let receipt = DownloadReceipt {
                version: 1,
                kind: "artifact-download".to_owned(),
                invocation_sha256: invocation,
                scope_sha256: download_scope_fingerprint(
                    endpoint,
                    session_id,
                    &arguments,
                ),
                session_id: hex(session_id.as_bytes()),
                transfer_id: hex(transfer_id.as_bytes()),
                artifact_id: hex(artifact_id.as_bytes()),
                request_seed: hex(seed.request_id.as_bytes()),
                correlation_seed: hex(seed.correlation_id.as_bytes()),
                temporary_sha256: path_fingerprint(&temporary),
                byte_size: None,
                content_sha256: None,
                media_type_sha256: None,
                verified_offset: 0,
                next_ordinal: 0,
                prefix_sha256: empty_sha256(),
                phase: DownloadPhase::OpenPrepared,
                output_delivered: false,
            };
            if let Some(path) = arguments.receipt.as_deref() {
                recovery::create(path, &receipt).await?;
            }
            receipt
        }
    };
    if receipt.session_id != hex(client.context().session_id().as_bytes()) {
        return Err(CliError::protocol(
            "resume artifact download",
            "daemon established a different durable session",
        ));
    }

    let transfer_id = stored_transfer(&receipt.transfer_id)?;
    ensure_distinct_paths(
        arguments.receipt.as_deref(),
        &temporary_path(&arguments.output, transfer_id),
        "--receipt conflicts with the artifact temporary",
    )
    .await?;
    let observed_media_type = loop {
        let identity = derived_identity(&receipt, b"download-open", 0)?;
        let response = match client
            .request(
                identity,
                AppRequestPayload::OpenArtifact(ArtifactOpenRequest::new(
                    transfer_id,
                    artifact_id,
                )),
            )
            .await
        {
            Ok(response) => response,
            Err(error) if reconnectable(&error) => {
                drop(client);
                client = reconnect_artifact(endpoint, &receipt.session_id, timeout).await?;
                continue;
            }
            Err(error) => return Err(error),
        };
        let AppResponsePayload::ArtifactOpened(metadata) = response.payload() else {
            return response_error(response.payload(), "artifact metadata").and_then(|()| {
                Err(CliError::protocol(
                    "artifact metadata",
                    "missing artifact-open response",
                ))
            });
        };
        validate_opened_metadata(metadata, transfer_id, artifact_id)?;
        bind_download_metadata(&mut receipt, metadata)?;
        receipt.phase = DownloadPhase::Receiving;
        persist(arguments.receipt.as_deref(), &receipt).await?;
        let media_type = metadata.media_type().as_str().to_owned();

        match receive_download(
            &mut client,
            metadata.clone(),
            &temporary_path(&arguments.output, transfer_id),
            &mut receipt,
            arguments.receipt.as_deref(),
        )
        .await
        {
            Ok(()) => {
                receipt.phase = DownloadPhase::Verified;
                persist(arguments.receipt.as_deref(), &receipt).await?;
                break media_type;
            }
            Err(error) if reconnectable(&error) => {
                drop(client);
                client = reconnect_artifact(endpoint, &receipt.session_id, timeout).await?;
            }
            Err(error) => return Err(error),
        }
    }
    drop(client);
    finish_download(
        &arguments,
        &mut receipt,
        arguments.receipt.as_deref(),
        output,
        Some(observed_media_type.as_str()),
        None,
    )
    .await
}

async fn receive_download(
    client: &mut Client,
    metadata: ArtifactMetadata,
    temporary: &Path,
    receipt: &mut DownloadReceipt,
    receipt_path: Option<&Path>,
) -> Result<(), CliError> {
    let (mut file, mut hasher) = open_download_partial(temporary, receipt).await?;
    let mut transfer =
        ArtifactTransferState::new(metadata.clone(), client.limits().max_artifact_chunk_bytes())
            .map_err(|error| {
                CliError::protocol("initialize artifact download", error.to_string())
            })?;
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
                if chunk.offset() > receipt.verified_offset {
                    return Err(CliError::protocol(
                        "stream artifact download",
                        "artifact stream skipped the durable local frontier",
                    ));
                }
                file.seek(SeekFrom::Start(chunk.offset()))
                    .await
                    .map_err(|error| {
                        CliError::local_io(
                            "seek temporary artifact output",
                            Some(temporary.to_path_buf()),
                            error,
                        )
                    })?;
                let retained_u64 = receipt.verified_offset.saturating_sub(chunk.offset());
                let retained = usize::try_from(retained_u64)
                    .unwrap_or(usize::MAX)
                    .min(chunk.bytes().len());
                if retained > 0 {
                    let mut existing = vec![0_u8; retained];
                    file.read_exact(&mut existing).await.map_err(|error| {
                        CliError::local_io(
                            "verify temporary artifact prefix",
                            Some(temporary.to_path_buf()),
                            error,
                        )
                    })?;
                    if existing != chunk.bytes()[..retained] {
                        return Err(CliError::protocol(
                            "verify temporary artifact prefix",
                            "replayed artifact bytes differ from the durable partial output",
                        ));
                    }
                }
                if retained < chunk.bytes().len() {
                    let suffix = &chunk.bytes()[retained..];
                    file.write_all(suffix).await.map_err(|error| {
                        CliError::local_io(
                            "write temporary artifact output",
                            Some(temporary.to_path_buf()),
                            error,
                        )
                    })?;
                    hasher.update(suffix);
                    file.flush().await.map_err(|error| {
                        CliError::local_io(
                            "flush temporary artifact output",
                            Some(temporary.to_path_buf()),
                            error,
                        )
                    })?;
                    file.sync_data().await.map_err(|error| {
                        CliError::local_io(
                            "sync temporary artifact output",
                            Some(temporary.to_path_buf()),
                            error,
                        )
                    })?;
                    receipt.verified_offset = chunk
                        .offset()
                        .checked_add(u64::try_from(chunk.bytes().len()).map_err(|_| {
                            CliError::runtime(
                                "count artifact bytes",
                                "chunk length does not fit the durable offset",
                            )
                        })?)
                        .ok_or_else(|| {
                            CliError::runtime(
                                "count artifact bytes",
                                "download offset overflow",
                            )
                        })?;
                    receipt.next_ordinal = chunk.ordinal().checked_add(1).ok_or_else(|| {
                        CliError::runtime(
                            "count artifact chunks",
                            "download ordinal overflow",
                        )
                    })?;
                    receipt.prefix_sha256 = digest_hasher(&hasher);
                    persist(receipt_path, receipt).await?;
                }
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
                if receipt.verified_offset != metadata.byte_size() {
                    return Err(CliError::protocol(
                        "complete artifact download",
                        "completion arrived before the durable local byte frontier",
                    ));
                }
                let digest = digest_hasher_bytes(&hasher);
                transfer.complete(Sha256Digest::new(digest)).map_err(|error| {
                    CliError::protocol("complete artifact download", error.to_string())
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

async fn open_download_partial(
    temporary: &Path,
    receipt: &DownloadReceipt,
) -> Result<(tokio::fs::File, Sha256), CliError> {
    match tokio::fs::symlink_metadata(temporary).await {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(CliError::local_io(
                "open temporary artifact output",
                Some(temporary.to_path_buf()),
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "temporary artifact path is not a regular file",
                ),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(CliError::local_io(
                "inspect temporary artifact output",
                Some(temporary.to_path_buf()),
                error,
            ));
        }
    }
    let mut options = tokio::fs::OpenOptions::new();
    options.create(true).read(true).write(true);
    let mut file = options.open(temporary).await.map_err(|error| {
        CliError::local_io(
            "open temporary artifact output",
            Some(temporary.to_path_buf()),
            error,
        )
    })?;
    recovery::sync_directory(temporary).await?;
    let length = file
        .metadata()
        .await
        .map_err(|error| {
            CliError::local_io(
                "inspect temporary artifact output",
                Some(temporary.to_path_buf()),
                error,
            )
        })?
        .len();
    if length < receipt.verified_offset {
        return Err(CliError::local_io(
            "verify temporary artifact output",
            Some(temporary.to_path_buf()),
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "temporary output is shorter than the durable download frontier",
            ),
        ));
    }
    if length > receipt.verified_offset {
        file.set_len(receipt.verified_offset).await.map_err(|error| {
            CliError::local_io(
                "truncate uncheckpointed artifact output",
                Some(temporary.to_path_buf()),
                error,
            )
        })?;
        file.sync_all().await.map_err(|error| {
            CliError::local_io(
                "sync truncated artifact output",
                Some(temporary.to_path_buf()),
                error,
            )
        })?;
    }
    file.seek(SeekFrom::Start(0)).await.map_err(|error| {
        CliError::local_io(
            "seek temporary artifact output",
            Some(temporary.to_path_buf()),
            error,
        )
    })?;
    let mut hasher = Sha256::new();
    let mut remaining = receipt.verified_offset;
    let mut buffer = vec![0_u8; HASH_BUFFER_BYTES];
    while remaining > 0 {
        let maximum = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let read = file.read(&mut buffer[..maximum]).await.map_err(|error| {
            CliError::local_io(
                "verify temporary artifact output",
                Some(temporary.to_path_buf()),
                error,
            )
        })?;
        if read == 0 {
            return Err(CliError::local_io(
                "verify temporary artifact output",
                Some(temporary.to_path_buf()),
                std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "temporary artifact ended before its durable frontier",
                ),
            ));
        }
        hasher.update(&buffer[..read]);
        remaining -= u64::try_from(read)
            .map_err(|_| CliError::runtime("count artifact bytes", "read length overflow"))?;
    }
    if digest_hasher(&hasher) != receipt.prefix_sha256 {
        return Err(CliError::local_io(
            "verify temporary artifact output",
            Some(temporary.to_path_buf()),
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "temporary bytes differ from their durable prefix digest",
            ),
        ));
    }
    Ok((file, hasher))
}

async fn finish_download(
    arguments: &ArtifactGetArgs,
    receipt: &mut DownloadReceipt,
    receipt_path: Option<&Path>,
    output: &Output,
    media_type: Option<&str>,
    prior_output: Option<&str>,
) -> Result<(), CliError> {
    let transfer = stored_transfer(&receipt.transfer_id)?;
    let temporary = temporary_path(&arguments.output, transfer);
    if receipt.temporary_sha256 != path_fingerprint(&temporary) {
        return Err(CliError::runtime(
            "validate operation receipt",
            "download receipt names a different temporary output",
        ));
    }
    if receipt.phase == DownloadPhase::Verified {
        reconcile_publication(&temporary, &arguments.output, arguments.force, receipt).await?;
        receipt.phase = DownloadPhase::Published;
        persist(receipt_path, receipt).await?;
    } else {
        verify_download_output(&arguments.output, receipt).await?;
    }
    let byte_size = receipt.byte_size.ok_or_else(|| {
        CliError::runtime(
            "validate operation receipt",
            "verified download omitted its byte size",
        )
    })?;
    let content = receipt.content_sha256.as_deref().ok_or_else(|| {
        CliError::runtime(
            "validate operation receipt",
            "verified download omitted its content digest",
        )
    })?;
    output.success(
        "artifact-downloaded",
        serde_json::json!({
            "artifact_id": receipt.artifact_id.as_str(),
            "transfer_id": receipt.transfer_id.as_str(),
            "bytes": byte_size,
            "sha256": content,
            "media_type": media_type,
            "media_type_sha256": receipt.media_type_sha256.as_deref(),
            "output": arguments.output.to_string_lossy(),
            "receipt": receipt_path.map(|path| path.display().to_string()),
            "prior_output": prior_output,
        }),
        &format!(
            "artifact {} downloaded to {} ({} bytes, sha256={}){}",
            receipt.artifact_id,
            arguments.output.display(),
            byte_size,
            content,
            prior_output
                .map(|state| format!("; prior output {state}"))
                .unwrap_or_default(),
        ),
    )?;
    receipt.output_delivered = true;
    receipt.phase = DownloadPhase::Reported;
    persist(receipt_path, receipt).await
}

async fn reconcile_publication(
    temporary: &Path,
    destination: &Path,
    force: bool,
    receipt: &DownloadReceipt,
) -> Result<(), CliError> {
    let temporary_metadata = tokio::fs::symlink_metadata(temporary).await;
    match temporary_metadata {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(CliError::local_io(
                "publish artifact output",
                Some(temporary.to_path_buf()),
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "verified artifact temporary is not a regular file",
                ),
            ));
        }
        Ok(_) if force => {
            verify_download_temporary(temporary, receipt).await?;
            recovery::publish_replace(temporary, destination).await?;
        }
        Ok(temporary_metadata) => {
            verify_download_temporary(temporary, receipt).await?;
            match tokio::fs::symlink_metadata(destination).await {
            Ok(destination_metadata)
                if same_physical_file(&temporary_metadata, &destination_metadata) =>
            {
                recovery::remove_durable(temporary).await?;
            }
            Ok(_) => {
                return Err(CliError::local_io(
                    "publish artifact output",
                    Some(destination.to_path_buf()),
                    std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "output already exists",
                    ),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                recovery::publish_new(temporary, destination).await?;
            }
            Err(error) => {
                return Err(CliError::local_io(
                    "inspect artifact output",
                    Some(destination.to_path_buf()),
                    error,
                ));
            }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            verify_download_output(destination, receipt).await?;
        }
        Err(error) => {
            return Err(CliError::local_io(
                "inspect temporary artifact output",
                Some(temporary.to_path_buf()),
                error,
            ));
        }
    }
    verify_download_output(destination, receipt).await
}

async fn verify_download_temporary(
    path: &Path,
    receipt: &DownloadReceipt,
) -> Result<(), CliError> {
    let expected_size = receipt.byte_size.ok_or_else(|| {
        invalid_receipt("verified download omitted its byte size")
    })?;
    let expected_digest = receipt.content_sha256.as_deref().ok_or_else(|| {
        invalid_receipt("verified download omitted its content digest")
    })?;
    let (size, digest) = digest_path(path, "verify temporary artifact output").await?;
    if size != expected_size || hex(digest.as_bytes()) != expected_digest {
        return Err(CliError::local_io(
            "verify temporary artifact output",
            Some(path.to_path_buf()),
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "temporary output differs from the verified artifact receipt",
            ),
        ));
    }
    Ok(())
}

async fn verify_download_output(path: &Path, receipt: &DownloadReceipt) -> Result<(), CliError> {
    let metadata = tokio::fs::symlink_metadata(path).await.map_err(|error| {
        CliError::local_io(
            "inspect published artifact output",
            Some(path.to_path_buf()),
            error,
        )
    })?;
    if !metadata.file_type().is_file() {
        return Err(CliError::local_io(
            "verify published artifact output",
            Some(path.to_path_buf()),
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "published artifact output is not a regular file",
            ),
        ));
    }
    let expected_size = receipt.byte_size.ok_or_else(|| {
        CliError::runtime(
            "validate operation receipt",
            "verified download omitted its byte size",
        )
    })?;
    let expected_digest = receipt.content_sha256.as_deref().ok_or_else(|| {
        CliError::runtime(
            "validate operation receipt",
            "verified download omitted its content digest",
        )
    })?;
    let (size, digest) = digest_path(path, "verify published artifact output").await?;
    if size != expected_size || hex(digest.as_bytes()) != expected_digest {
        return Err(CliError::local_io(
            "verify published artifact output",
            Some(path.to_path_buf()),
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "published output differs from the verified artifact receipt",
            ),
        ));
    }
    Ok(())
}

pub async fn put(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
    arguments: ArtifactPutArgs,
    output: &Output,
) -> Result<(), CliError> {
    ensure_distinct_paths(
        arguments.receipt.as_deref(),
        &arguments.input,
        "--receipt and --input must name different files",
    )
    .await?;
    let _receipt_lease = recovery::lease(arguments.receipt.as_deref()).await?;
    let artifact_id = ArtifactId::new(arguments.artifact)
        .map_err(|_| CliError::usage("invalid --artifact identifier"))?;
    let invocation = upload_invocation_fingerprint(endpoint, &arguments);
    let retained = match arguments.receipt.as_deref() {
        Some(path) => recovery::load::<UploadReceipt>(path).await?,
        None => None,
    };
    if let Some(receipt) = &retained {
        validate_upload_header(receipt, &invocation, session, &arguments)?;
        if receipt.phase == UploadPhase::Settled {
            let mut receipt = receipt.clone();
            let prior_output = if receipt.output_delivered {
                Some("confirmed")
            } else {
                Some("possible")
            };
            return render_upload(
                &mut receipt,
                arguments.receipt.as_deref(),
                output,
                prior_output,
            )
            .await;
        }
    }

    let mut source = inspect_source(&arguments.input).await?;
    let requested_session = retained
        .as_ref()
        .map(|receipt| stored_session(&receipt.session_id))
        .transpose()?
        .or(session);
    let mut client = connect_artifact(endpoint, requested_session, timeout).await?;
    let chunk_size = usize::try_from(arguments.chunk_size)
        .map_err(|_| CliError::usage("--chunk-size cannot be represented on this platform"))?;
    validate_chunk_size(chunk_size, &client)?;
    let media_type = CanonicalMediaType::new(
        arguments.media_type.clone(),
        client.limits().codec().max_string_bytes,
    )
    .map_err(|error| CliError::usage(error.to_string()))?;

    let mut receipt = match retained {
        Some(receipt) => {
            validate_upload_source(&receipt, endpoint, &arguments, &source)?;
            validate_pending_source(&mut source, &receipt, chunk_size).await?;
            receipt
        }
        None => {
            let transfer_id = fresh_transfer_id()?;
            let seed = Client::new_request_identity()?;
            let session_id = client.context().session_id();
            let receipt = UploadReceipt {
                version: 1,
                kind: "artifact-upload".to_owned(),
                invocation_sha256: invocation,
                scope_sha256: upload_scope_fingerprint(
                    endpoint,
                    session_id,
                    &arguments,
                    &source,
                ),
                session_id: hex(session_id.as_bytes()),
                transfer_id: hex(transfer_id.as_bytes()),
                artifact_id: hex(artifact_id.as_bytes()),
                source_identity_sha256: source.identity_sha256.clone(),
                byte_size: source.byte_size,
                content_sha256: hex(source.digest.as_bytes()),
                request_seed: hex(seed.request_id.as_bytes()),
                correlation_seed: hex(seed.correlation_id.as_bytes()),
                acknowledged_offset: 0,
                next_ordinal: 0,
                pending: None,
                phase: UploadPhase::BeginPrepared,
                output_delivered: false,
            };
            if let Some(path) = arguments.receipt.as_deref() {
                recovery::create(path, &receipt).await?;
            }
            receipt
        }
    };
    if receipt.session_id != hex(client.context().session_id().as_bytes()) {
        return Err(CliError::protocol(
            "resume artifact upload",
            "daemon established a different durable session",
        ));
    }
    let transfer_id = stored_transfer(&receipt.transfer_id)?;
    let metadata = ArtifactMetadata::new(
        transfer_id,
        artifact_id,
        receipt.byte_size,
        media_type,
        stored_digest(&receipt.content_sha256, "content")?,
        arguments.chunk_size,
        client.limits().max_artifact_chunk_bytes(),
    )
    .map_err(|error| CliError::usage(error.to_string()))?;

    loop {
        validate_chunk_size(chunk_size, &client)?;
        if arguments.media_type.len() > client.limits().codec().max_string_bytes {
            return Err(CliError::usage(
                "--media-type exceeds the currently negotiated string limit",
            ));
        }
        validate_chunk_size(chunk_size, &client)?;
        if arguments.media_type.len() > client.limits().codec().max_string_bytes {
            return Err(CliError::usage(
                "--media-type exceeds the currently negotiated string limit",
            ));
        }
        let reconciling_completion = receipt.phase == UploadPhase::CompletionPrepared;
        if reconciling_completion {
            match probe_uploaded_artifact(
                &mut client,
                &receipt,
                artifact_id,
                &arguments.media_type,
            )
            .await
            {
                Ok(true) => {
                    receipt.phase = UploadPhase::Settled;
                    persist(arguments.receipt.as_deref(), &receipt).await?;
                    drop(client);
                    return render_upload(
                        &mut receipt,
                        arguments.receipt.as_deref(),
                        output,
                        None,
                    )
                    .await;
                }
                Ok(false) => {
                    return Err(CliError::protocol(
                        "reconcile artifact upload",
                        "artifact reconciliation returned no terminal observation",
                    ));
                }
                Err(error) if reconnectable(&error) => {
                    drop(client);
                    client = reconnect_artifact(endpoint, &receipt.session_id, timeout).await?;
                    continue;
                }
                Err(error) => return Err(error),
            }
        }

        if !reconciling_completion {
            receipt.phase = UploadPhase::BeginPrepared;
            persist(arguments.receipt.as_deref(), &receipt).await?;
        }
        let begin_identity = derived_identity(&receipt, b"upload-begin", 0)?;
        let response = match client
            .request(
                begin_identity,
                AppRequestPayload::BeginArtifactUpload(metadata.clone()),
            )
            .await
        {
            Ok(response) => response,
            Err(error) if reconnectable(&error) => {
                drop(client);
                client = reconnect_artifact(endpoint, &receipt.session_id, timeout).await?;
                continue;
            }
            Err(error) => return Err(error),
        };
        expect_ack(response.payload(), begin_identity, "begin artifact upload")?;
        receipt.phase = UploadPhase::Uploading;
        persist(arguments.receipt.as_deref(), &receipt).await?;

        match upload_chunks(
            &mut client,
            &mut source,
            transfer_id,
            artifact_id,
            chunk_size,
            &mut receipt,
            arguments.receipt.as_deref(),
        )
        .await
        {
            Ok(()) => {}
            Err(error) if reconnectable(&error) => {
                drop(client);
                client = reconnect_artifact(endpoint, &receipt.session_id, timeout).await?;
                continue;
            }
            Err(error) => return Err(error),
        }

        receipt.phase = UploadPhase::CompletionPrepared;
        receipt.pending = None;
        persist(arguments.receipt.as_deref(), &receipt).await?;
        let completion_identity = derived_identity(&receipt, b"upload-complete", 0)?;
        let completion = ArtifactCompletion::new(
            transfer_id,
            artifact_id,
            receipt.byte_size,
            stored_digest(&receipt.content_sha256, "content")?,
        );
        let response = match client
            .request(
                completion_identity,
                AppRequestPayload::CompleteArtifactUpload(completion),
            )
            .await
        {
            Ok(response) => response,
            Err(error) if reconnectable(&error) => {
                drop(client);
                client = reconnect_artifact(endpoint, &receipt.session_id, timeout).await?;
                continue;
            }
            Err(error) => return Err(error),
        };
        if let AppResponsePayload::Error(error) = response.payload() {
            receipt.phase = UploadPhase::BeginPrepared;
            persist(arguments.receipt.as_deref(), &receipt).await?;
            return Err(CliError::rejected(error));
        }
        expect_ack(
            response.payload(),
            completion_identity,
            "complete artifact upload",
        )?;
        receipt.phase = UploadPhase::Settled;
        persist(arguments.receipt.as_deref(), &receipt).await?;
        drop(client);
        return render_upload(
            &mut receipt,
            arguments.receipt.as_deref(),
            output,
            None,
        )
        .await;
    }
}

async fn upload_chunks(
    client: &mut Client,
    source: &mut SourceSnapshot,
    transfer_id: TransferId,
    artifact_id: ArtifactId,
    chunk_size: usize,
    receipt: &mut UploadReceipt,
    receipt_path: Option<&Path>,
) -> Result<(), CliError> {
    source.file.seek(SeekFrom::Start(0)).await.map_err(|error| {
        CliError::local_io(
            "seek artifact input",
            Some(source.path.clone()),
            error,
        )
    })?;
    let mut buffer = vec![0_u8; chunk_size];
    let mut ordinal = 0_u64;
    let mut offset = 0_u64;
    let mut hasher = Sha256::new();
    loop {
        let read = source.file.read(&mut buffer).await.map_err(|error| {
            CliError::local_io("read artifact input", Some(source.path.clone()), error)
        })?;
        if read == 0 {
            break;
        }
        let bytes = &buffer[..read];
        hasher.update(bytes);
        let read_u64 = u64::try_from(read)
            .map_err(|_| CliError::runtime("count artifact bytes", "chunk length overflow"))?;
        let end = offset
            .checked_add(read_u64)
            .ok_or_else(|| CliError::runtime("count artifact bytes", "byte offset overflow"))?;
        if end > receipt.byte_size {
            return Err(source_changed_error(&source.path));
        }
        receipt.pending = Some(PendingChunk {
            ordinal,
            offset,
            byte_size: read_u64,
            sha256: digest_bytes(bytes),
        });
        persist(receipt_path, receipt).await?;
        let chunk = ArtifactChunk::new(
            transfer_id,
            artifact_id,
            ordinal,
            offset,
            bytes.to_vec(),
            client.limits().max_artifact_chunk_bytes(),
        )
        .map_err(|error| CliError::protocol("construct artifact chunk", error.to_string()))?;
        let identity = derived_identity(receipt, b"upload-chunk", ordinal)?;
        let response = client
            .request(identity, AppRequestPayload::UploadArtifactChunk(chunk))
            .await?;
        expect_ack(response.payload(), identity, "upload artifact chunk")?;
        if end > receipt.acknowledged_offset {
            receipt.acknowledged_offset = end;
            receipt.next_ordinal = ordinal.checked_add(1).ok_or_else(|| {
                CliError::runtime("count artifact chunks", "chunk ordinal overflow")
            })?;
        }
        receipt.pending = None;
        persist(receipt_path, receipt).await?;
        offset = end;
        ordinal = ordinal
            .checked_add(1)
            .ok_or_else(|| CliError::runtime("count artifact chunks", "chunk ordinal overflow"))?;
    }
    let observed: [u8; 32] = hasher.finalize().into();
    if offset != receipt.byte_size
        || observed != *stored_digest(&receipt.content_sha256, "content")?.as_bytes()
    {
        return Err(source_changed_error(&source.path));
    }
    let metadata = source.file.metadata().await.map_err(|error| {
        CliError::local_io("inspect artifact input", Some(source.path.clone()), error)
    })?;
    if source_identity_fingerprint(&source.path, &metadata) != receipt.source_identity_sha256 {
        return Err(source_changed_error(&source.path));
    }
    receipt.acknowledged_offset = offset;
    receipt.next_ordinal = ordinal;
    receipt.pending = None;
    persist(receipt_path, receipt).await
}

async fn probe_uploaded_artifact(
    client: &mut Client,
    receipt: &UploadReceipt,
    artifact_id: ArtifactId,
    media_type: &str,
) -> Result<bool, CliError> {
    let probe_transfer = derived_transfer(receipt, b"upload-probe-transfer", 0)?;
    let identity = derived_identity(receipt, b"upload-probe", 0)?;
    let response = client
        .request(
            identity,
            AppRequestPayload::OpenArtifact(ArtifactOpenRequest::new(probe_transfer, artifact_id)),
        )
        .await?;
    match response.payload() {
        AppResponsePayload::ArtifactOpened(metadata) => {
            if metadata.transfer_id() != probe_transfer
                || metadata.artifact_id() != artifact_id
                || metadata.byte_size() != receipt.byte_size
                || hex(metadata.digest().as_bytes()) != receipt.content_sha256
                || metadata.media_type().as_str() != media_type
            {
                return Err(CliError::protocol(
                    "reconcile artifact upload",
                    "published artifact metadata differs from the original upload receipt",
                ));
            }
            Ok(true)
        }
        AppResponsePayload::Error(error) if error.code() == AppErrorCode::ArtifactState => Ok(false),
        payload => response_error(payload, "reconcile artifact upload").and(Ok(false)),
    }
}

async fn render_upload(
    receipt: &mut UploadReceipt,
    receipt_path: Option<&Path>,
    output: &Output,
    prior_output: Option<&str>,
) -> Result<(), CliError> {
    output.success(
        "artifact-uploaded",
        serde_json::json!({
            "artifact_id": receipt.artifact_id.as_str(),
            "transfer_id": receipt.transfer_id.as_str(),
            "bytes": receipt.byte_size,
            "sha256": receipt.content_sha256.as_str(),
            "chunks": receipt.next_ordinal,
            "acknowledged_offset": receipt.acknowledged_offset,
            "receipt": receipt_path.map(|path| path.display().to_string()),
            "prior_output": prior_output,
        }),
        &format!(
            "artifact {} uploaded ({} bytes, {} chunks, sha256={}){}",
            receipt.artifact_id,
            receipt.byte_size,
            receipt.next_ordinal,
            receipt.content_sha256,
            prior_output
                .map(|state| format!("; prior output {state}"))
                .unwrap_or_default(),
        ),
    )?;
    receipt.output_delivered = true;
    persist(receipt_path, receipt).await
}

async fn inspect_source(path: &Path) -> Result<SourceSnapshot, CliError> {
    let mut file = tokio::fs::File::open(path).await.map_err(|error| {
        CliError::local_io("open artifact input", Some(path.to_path_buf()), error)
    })?;
    let before = file.metadata().await.map_err(|error| {
        CliError::local_io("inspect artifact input", Some(path.to_path_buf()), error)
    })?;
    if !before.is_file() {
        return Err(CliError::local_io(
            "inspect artifact input",
            Some(path.to_path_buf()),
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "artifact input is not a regular file",
            ),
        ));
    }
    let identity = source_identity_fingerprint(path, &before);
    let (byte_size, digest) = digest_open_file(&mut file, path, "read artifact input").await?;
    let after = file.metadata().await.map_err(|error| {
        CliError::local_io("inspect artifact input", Some(path.to_path_buf()), error)
    })?;
    if byte_size != before.len()
        || before.len() != after.len()
        || identity != source_identity_fingerprint(path, &after)
    {
        return Err(source_changed_error(path));
    }
    file.seek(SeekFrom::Start(0)).await.map_err(|error| {
        CliError::local_io("seek artifact input", Some(path.to_path_buf()), error)
    })?;
    Ok(SourceSnapshot {
        path: path.to_path_buf(),
        file,
        byte_size,
        digest,
        identity_sha256: identity,
    })
}

async fn digest_path(
    path: &Path,
    operation: &'static str,
) -> Result<(u64, Sha256Digest), CliError> {
    let mut file = tokio::fs::File::open(path).await.map_err(|error| {
        CliError::local_io(operation, Some(path.to_path_buf()), error)
    })?;
    digest_open_file(&mut file, path, operation).await
}

async fn digest_open_file(
    file: &mut tokio::fs::File,
    path: &Path,
    operation: &'static str,
) -> Result<(u64, Sha256Digest), CliError> {
    file.seek(SeekFrom::Start(0)).await.map_err(|error| {
        CliError::local_io(operation, Some(path.to_path_buf()), error)
    })?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; HASH_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).await.map_err(|error| {
            CliError::local_io(operation, Some(path.to_path_buf()), error)
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total = total
            .checked_add(
                u64::try_from(read).map_err(|_| {
                    CliError::runtime("count artifact bytes", "read length overflow")
                })?,
            )
            .ok_or_else(|| CliError::runtime("count artifact bytes", "file size overflow"))?;
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Ok((total, Sha256Digest::new(digest)))
}

fn bind_download_metadata(
    receipt: &mut DownloadReceipt,
    metadata: &ArtifactMetadata,
) -> Result<(), CliError> {
    let content = hex(metadata.digest().as_bytes());
    let media = digest_bytes(metadata.media_type().as_str().as_bytes());
    match (
        receipt.byte_size,
        receipt.content_sha256.as_deref(),
        receipt.media_type_sha256.as_deref(),
    ) {
        (None, None, None) => {
            receipt.byte_size = Some(metadata.byte_size());
            receipt.content_sha256 = Some(content);
            receipt.media_type_sha256 = Some(media);
            Ok(())
        }
        (Some(size), Some(digest), Some(media_digest))
            if size == metadata.byte_size() && digest == content && media_digest == media =>
        {
            Ok(())
        }
        _ => Err(CliError::protocol(
            "resume artifact download",
            "reopened artifact metadata differs from the durable receipt",
        )),
    }
}

fn validate_opened_metadata(
    metadata: &ArtifactMetadata,
    transfer_id: TransferId,
    artifact_id: ArtifactId,
) -> Result<(), CliError> {
    if metadata.transfer_id() != transfer_id || metadata.artifact_id() != artifact_id {
        return Err(CliError::protocol(
            "validate artifact metadata",
            "daemon opened a different transfer or artifact",
        ));
    }
    Ok(())
}

fn validate_download_receipt(
    receipt: &DownloadReceipt,
    invocation: &str,
    endpoint: &OsStr,
    arguments: &ArtifactGetArgs,
    requested_session: Option<SessionId>,
) -> Result<(), CliError> {
    if receipt.version != 1 || receipt.kind != "artifact-download" {
        return Err(invalid_receipt("receipt is not a supported artifact-download receipt"));
    }
    if receipt.invocation_sha256 != invocation
        || receipt.artifact_id != hex(&arguments.artifact)
    {
        return Err(CliError::usage(
            "--receipt belongs to a different artifact download invocation",
        ));
    }
    let session = stored_session(&receipt.session_id)?;
    if requested_session.is_some() && requested_session != Some(session) {
        return Err(CliError::usage(
            "--session does not match the durable session in --receipt",
        ));
    }
    if receipt.scope_sha256 != download_scope_fingerprint(endpoint, session, arguments) {
        return Err(invalid_receipt("download receipt scope fingerprint is inconsistent"));
    }
    let transfer = stored_transfer(&receipt.transfer_id)?;
    let temporary = temporary_path(&arguments.output, transfer);
    if receipt.temporary_sha256 != path_fingerprint(&temporary) {
        return Err(invalid_receipt("download receipt temporary path is inconsistent"));
    }
    stored_seed(&receipt.request_seed, "request seed")?;
    stored_seed(&receipt.correlation_seed, "correlation seed")?;
    if receipt.prefix_sha256.len() != 64
        || receipt.verified_offset > receipt.byte_size.unwrap_or(u64::MAX)
        || matches!(
            receipt.phase,
            DownloadPhase::Verified | DownloadPhase::Published | DownloadPhase::Reported
        ) && receipt.byte_size != Some(receipt.verified_offset)
    {
        return Err(invalid_receipt("download receipt contains inconsistent frontiers"));
    }
    stored_digest(&receipt.prefix_sha256, "download prefix")?;
    if receipt.phase == DownloadPhase::Reported && !receipt.output_delivered {
        return Err(invalid_receipt(
            "reported download omitted its physical output frontier",
        ));
    }
    match (
        receipt.byte_size,
        receipt.content_sha256.as_deref(),
        receipt.media_type_sha256.as_deref(),
    ) {
        (None, None, None)
            if receipt.phase == DownloadPhase::OpenPrepared
                && receipt.verified_offset == 0
                && receipt.next_ordinal == 0 =>
        {
            Ok(())
        }
        (Some(_), Some(content), Some(media)) if content.len() == 64 && media.len() == 64 => {
            stored_digest(content, "download content")?;
            stored_digest(media, "download media type")?;
            Ok(())
        }
        _ => Err(invalid_receipt("download receipt contains incomplete metadata")),
    }
}

fn validate_upload_header(
    receipt: &UploadReceipt,
    invocation: &str,
    requested_session: Option<SessionId>,
    arguments: &ArtifactPutArgs,
) -> Result<(), CliError> {
    if receipt.version != 1 || receipt.kind != "artifact-upload" {
        return Err(invalid_receipt("receipt is not a supported artifact-upload receipt"));
    }
    if receipt.invocation_sha256 != invocation {
        return Err(CliError::usage(
            "--receipt belongs to a different artifact upload invocation",
        ));
    }
    let session = stored_session(&receipt.session_id)?;
    if requested_session.is_some() && requested_session != Some(session) {
        return Err(CliError::usage(
            "--session does not match the durable session in --receipt",
        ));
    }
    stored_transfer(&receipt.transfer_id)?;
    stored_seed(&receipt.request_seed, "request seed")?;
    stored_seed(&receipt.correlation_seed, "correlation seed")?;
    if receipt.content_sha256.len() != 64
        || receipt.source_identity_sha256.len() != 64
        || receipt.acknowledged_offset > receipt.byte_size
        || receipt.phase == UploadPhase::CompletionPrepared
            && receipt.acknowledged_offset != receipt.byte_size
        || receipt.phase == UploadPhase::Settled
            && receipt.acknowledged_offset != receipt.byte_size
    {
        return Err(invalid_receipt("upload receipt contains inconsistent frontiers"));
    }
    stored_digest(&receipt.content_sha256, "upload content")?;
    stored_digest(&receipt.source_identity_sha256, "source identity")?;
    if matches!(receipt.phase, UploadPhase::CompletionPrepared | UploadPhase::Settled)
        && receipt.pending.is_some()
    {
        return Err(invalid_receipt(
            "terminal upload frontier retained a pending chunk",
        ));
    }
    let chunk_size = u64::from(arguments.chunk_size);
    let expected_frontier = receipt
        .next_ordinal
        .checked_mul(chunk_size)
        .unwrap_or(u64::MAX)
        .min(receipt.byte_size);
    if expected_frontier != receipt.acknowledged_offset {
        return Err(invalid_receipt(
            "upload receipt ordinal and acknowledged offset disagree",
        ));
    }
    if let Some(pending) = &receipt.pending {
        if pending.sha256.len() != 64
            || pending.offset > receipt.byte_size
            || pending.byte_size > receipt.byte_size.saturating_sub(pending.offset)
            || pending.offset
                != pending
                    .ordinal
                    .checked_mul(chunk_size)
                    .unwrap_or(u64::MAX)
            || pending.byte_size
                != chunk_size.min(receipt.byte_size.saturating_sub(pending.offset))
        {
            return Err(invalid_receipt("upload receipt contains an invalid pending chunk"));
        }
        stored_digest(&pending.sha256, "pending chunk")?;
    }
    Ok(())
}

fn validate_upload_source(
    receipt: &UploadReceipt,
    endpoint: &OsStr,
    arguments: &ArtifactPutArgs,
    source: &SourceSnapshot,
) -> Result<(), CliError> {
    let session = stored_session(&receipt.session_id)?;
    if receipt.artifact_id != hex(&arguments.artifact)
        || receipt.source_identity_sha256 != source.identity_sha256
        || receipt.byte_size != source.byte_size
        || receipt.content_sha256 != hex(source.digest.as_bytes())
        || receipt.scope_sha256
            != upload_scope_fingerprint(endpoint, session, arguments, source)
    {
        return Err(CliError::usage(
            "--receipt belongs to different source bytes, source identity, or upload scope",
        ));
    }
    Ok(())
}

async fn validate_pending_source(
    source: &mut SourceSnapshot,
    receipt: &UploadReceipt,
    chunk_size: usize,
) -> Result<(), CliError> {
    let Some(pending) = &receipt.pending else {
        return Ok(());
    };
    let length = usize::try_from(pending.byte_size)
        .map_err(|_| invalid_receipt("pending chunk length does not fit this platform"))?;
    if length == 0 || length > chunk_size {
        return Err(invalid_receipt(
            "pending chunk length exceeds the physical chunk size",
        ));
    }
    source
        .file
        .seek(SeekFrom::Start(pending.offset))
        .await
        .map_err(|error| {
            CliError::local_io("seek artifact input", Some(source.path.clone()), error)
        })?;
    let mut bytes = vec![0_u8; length];
    source.file.read_exact(&mut bytes).await.map_err(|error| {
        CliError::local_io("verify pending artifact chunk", Some(source.path.clone()), error)
    })?;
    if digest_bytes(&bytes) != pending.sha256 {
        return Err(source_changed_error(&source.path));
    }
    source.file.seek(SeekFrom::Start(0)).await.map_err(|error| {
        CliError::local_io("seek artifact input", Some(source.path.clone()), error)
    })?;
    Ok(())
}

fn download_invocation_fingerprint(endpoint: &OsStr, arguments: &ArtifactGetArgs) -> String {
    let mut hasher = scope_hasher(b"peritus/cli-artifact-download-invocation/v1\0");
    fingerprint_part(&mut hasher, endpoint.as_encoded_bytes());
    fingerprint_part(&mut hasher, &arguments.artifact);
    fingerprint_part(&mut hasher, arguments.output.as_os_str().as_encoded_bytes());
    hasher.update([u8::from(arguments.force)]);
    finish_fingerprint(hasher)
}

fn download_scope_fingerprint(
    endpoint: &OsStr,
    session: SessionId,
    arguments: &ArtifactGetArgs,
) -> String {
    let mut hasher = scope_hasher(b"peritus/cli-artifact-download-scope/v1\0");
    fingerprint_part(&mut hasher, endpoint.as_encoded_bytes());
    fingerprint_part(&mut hasher, session.as_bytes());
    fingerprint_part(&mut hasher, &arguments.artifact);
    fingerprint_part(&mut hasher, arguments.output.as_os_str().as_encoded_bytes());
    hasher.update([u8::from(arguments.force)]);
    finish_fingerprint(hasher)
}

fn upload_invocation_fingerprint(endpoint: &OsStr, arguments: &ArtifactPutArgs) -> String {
    let mut hasher = scope_hasher(b"peritus/cli-artifact-upload-invocation/v1\0");
    fingerprint_part(&mut hasher, endpoint.as_encoded_bytes());
    fingerprint_part(&mut hasher, &arguments.artifact);
    fingerprint_part(&mut hasher, arguments.input.as_os_str().as_encoded_bytes());
    fingerprint_part(&mut hasher, arguments.media_type.as_bytes());
    hasher.update(arguments.chunk_size.to_be_bytes());
    finish_fingerprint(hasher)
}

fn upload_scope_fingerprint(
    endpoint: &OsStr,
    session: SessionId,
    arguments: &ArtifactPutArgs,
    source: &SourceSnapshot,
) -> String {
    let mut hasher = scope_hasher(b"peritus/cli-artifact-upload-scope/v1\0");
    fingerprint_part(&mut hasher, endpoint.as_encoded_bytes());
    fingerprint_part(&mut hasher, session.as_bytes());
    fingerprint_part(&mut hasher, &arguments.artifact);
    fingerprint_part(&mut hasher, arguments.input.as_os_str().as_encoded_bytes());
    fingerprint_part(&mut hasher, arguments.media_type.as_bytes());
    hasher.update(arguments.chunk_size.to_be_bytes());
    fingerprint_part(&mut hasher, source.identity_sha256.as_bytes());
    hasher.update(source.byte_size.to_be_bytes());
    fingerprint_part(&mut hasher, source.digest.as_bytes());
    finish_fingerprint(hasher)
}

fn source_identity_fingerprint(path: &Path, metadata: &std::fs::Metadata) -> String {
    let mut hasher = scope_hasher(b"peritus/cli-artifact-source-identity/v1\0");
    fingerprint_part(&mut hasher, path.as_os_str().as_encoded_bytes());
    hasher.update(metadata.len().to_be_bytes());
    fingerprint_time(&mut hasher, metadata.modified().ok());
    fingerprint_time(&mut hasher, metadata.created().ok());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        hasher.update(metadata.dev().to_be_bytes());
        hasher.update(metadata.ino().to_be_bytes());
    }
    finish_fingerprint(hasher)
}

fn fingerprint_time(hasher: &mut Sha256, value: Option<std::time::SystemTime>) {
    let Some(value) = value else {
        hasher.update([0]);
        return;
    };
    match value.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            hasher.update([1]);
            hasher.update(duration.as_secs().to_be_bytes());
            hasher.update(duration.subsec_nanos().to_be_bytes());
        }
        Err(error) => {
            hasher.update([2]);
            hasher.update(error.duration().as_secs().to_be_bytes());
            hasher.update(error.duration().subsec_nanos().to_be_bytes());
        }
    }
}

fn path_fingerprint(path: &Path) -> String {
    let mut hasher = scope_hasher(b"peritus/cli-artifact-path/v1\0");
    fingerprint_part(&mut hasher, path.as_os_str().as_encoded_bytes());
    finish_fingerprint(hasher)
}

fn scope_hasher(domain: &[u8]) -> Sha256 {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher
}

fn fingerprint_part(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn finish_fingerprint(hasher: Sha256) -> String {
    let digest: [u8; 32] = hasher.finalize().into();
    hex(&digest)
}

fn digest_bytes(bytes: &[u8]) -> String {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    hex(&digest)
}

fn digest_hasher(hasher: &Sha256) -> String {
    hex(&digest_hasher_bytes(hasher))
}

fn digest_hasher_bytes(hasher: &Sha256) -> [u8; 32] {
    hasher.clone().finalize().into()
}

fn empty_sha256() -> String {
    digest_bytes(&[])
}

fn derived_identity<R: IdentityReceipt>(
    receipt: &R,
    operation: &[u8],
    ordinal: u64,
) -> Result<RequestIdentity, CliError> {
    let request_seed = stored_seed(receipt.request_seed(), "request seed")?;
    let correlation_seed = stored_seed(receipt.correlation_seed(), "correlation seed")?;
    let request = RequestId::new(derive_id(
        b"peritus/cli-artifact-request/v1\0",
        request_seed,
        operation,
        ordinal,
    ))
    .map_err(|_| invalid_receipt("derived request identity is invalid"))?;
    let correlation = CorrelationId::new(derive_id(
        b"peritus/cli-artifact-correlation/v1\0",
        correlation_seed,
        operation,
        ordinal,
    ))
    .map_err(|_| invalid_receipt("derived correlation identity is invalid"))?;
    Ok(RequestIdentity::new(request, correlation))
}

fn derived_transfer<R: IdentityReceipt>(
    receipt: &R,
    operation: &[u8],
    ordinal: u64,
) -> Result<TransferId, CliError> {
    let seed = stored_seed(receipt.request_seed(), "request seed")?;
    TransferId::new(derive_id(
        b"peritus/cli-artifact-transfer/v1\0",
        seed,
        operation,
        ordinal,
    ))
    .map_err(|_| invalid_receipt("derived transfer identity is invalid"))
}

fn derive_id(domain: &[u8], seed: [u8; 16], operation: &[u8], ordinal: u64) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(seed);
    fingerprint_part(&mut hasher, operation);
    hasher.update(ordinal.to_be_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut identity = [0_u8; 16];
    identity.copy_from_slice(&digest[..16]);
    if identity == [0; 16] {
        identity[15] = 1;
    }
    identity
}

fn fresh_transfer_id() -> Result<TransferId, CliError> {
    let identity = Client::new_request_identity()?;
    TransferId::new(*identity.request_id.as_bytes())
        .map_err(|_| CliError::runtime("create transfer identity", "generated zero identifier"))
}

fn stored_session(value: &str) -> Result<SessionId, CliError> {
    SessionId::new(stored_seed(value, "session")?)
        .map_err(|_| invalid_receipt("receipt contains an invalid session identity"))
}

fn stored_transfer(value: &str) -> Result<TransferId, CliError> {
    TransferId::new(stored_seed(value, "transfer")?)
        .map_err(|_| invalid_receipt("receipt contains an invalid transfer identity"))
}

fn stored_seed(value: &str, field: &str) -> Result<[u8; 16], CliError> {
    parse_hex_id(value, field).map_err(|_| {
        invalid_receipt(&format!("receipt contains an invalid {field} identity"))
    })
}

fn stored_digest(value: &str, field: &str) -> Result<Sha256Digest, CliError> {
    parse_hex_digest(value, field)
        .map(Sha256Digest::new)
        .map_err(|_| invalid_receipt(&format!("receipt contains an invalid {field} digest")))
}

fn temporary_path(output: &Path, transfer: TransferId) -> PathBuf {
    let mut name = output
        .file_name()
        .map_or_else(|| "artifact".into(), std::ffi::OsString::from);
    name.push(format!(".peritus-{}.part", hex(transfer.as_bytes())));
    output.with_file_name(name)
}

fn source_changed_error(path: &Path) -> CliError {
    CliError::local_io(
        "verify artifact input",
        Some(path.to_path_buf()),
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "source identity or bytes changed after the upload receipt was prepared",
        ),
    )
}

fn invalid_receipt(detail: &str) -> CliError {
    CliError::runtime("validate operation receipt", detail.to_owned())
}

fn validate_chunk_size(chunk_size: usize, client: &Client) -> Result<(), CliError> {
    if chunk_size == 0 || chunk_size > client.limits().max_artifact_chunk_bytes() {
        return Err(CliError::usage(format!(
            "--chunk-size must be between 1 and {}",
            client.limits().max_artifact_chunk_bytes(),
        )));
    }
    Ok(())
}

fn expect_ack(
    payload: &AppResponsePayload,
    identity: RequestIdentity,
    operation: &'static str,
) -> Result<(), CliError> {
    match payload {
        AppResponsePayload::Acknowledged(acknowledgement)
            if acknowledgement.request_id() == identity.request_id =>
        {
            Ok(())
        }
        AppResponsePayload::Acknowledged(_) => Err(CliError::protocol(
            operation,
            "daemon acknowledged a different request identity",
        )),
        _ => response_error(payload, operation),
    }
}

async fn connect_artifact(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
) -> Result<Client, CliError> {
    Client::connect(
        endpoint,
        session,
        timeout,
        &[WellKnownProtocolFeature::ArtifactTransfer],
    )
    .await
}

async fn reconnect_artifact(
    endpoint: &OsStr,
    session: &str,
    timeout: Option<Duration>,
) -> Result<Client, CliError> {
    connect_artifact(endpoint, Some(stored_session(session)?), timeout).await
}

fn reconnectable(error: &CliError) -> bool {
    error.category() == ExitCategory::Connection
}

async fn ensure_distinct_paths(
    receipt: Option<&Path>,
    data: &Path,
    message: &'static str,
) -> Result<(), CliError> {
    let Some(receipt) = receipt else {
        return Ok(());
    };
    if receipt == data {
        return Err(CliError::usage(message));
    }
    if let (Ok(receipt_metadata), Ok(data_metadata)) = (
        tokio::fs::metadata(receipt).await,
        tokio::fs::metadata(data).await,
    ) && same_physical_file(&receipt_metadata, &data_metadata)
    {
        return Err(CliError::usage(message));
    }
    let receipt_key = resolved_path_key(receipt, "resolve operation receipt path").await?;
    let data_key = resolved_path_key(data, "resolve artifact data path").await?;
    if receipt_key == data_key {
        Err(CliError::usage(message))
    } else {
        Ok(())
    }
}

async fn resolved_path_key(
    path: &Path,
    operation: &'static str,
) -> Result<PathBuf, CliError> {
    match tokio::fs::canonicalize(path).await {
        Ok(path) => return Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(CliError::local_io(
                operation,
                Some(path.to_path_buf()),
                error,
            ));
        }
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    match tokio::fs::canonicalize(parent).await {
        Ok(parent) => Ok(path.file_name().map_or(parent.clone(), |name| parent.join(name))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let current = std::env::current_dir().map_err(|error| {
                CliError::local_io(operation, Some(path.to_path_buf()), error)
            })?;
            Ok(if path.is_absolute() {
                path.to_path_buf()
            } else {
                current.join(path)
            })
        }
        Err(error) => Err(CliError::local_io(
            operation,
            Some(parent.to_path_buf()),
            error,
        )),
    }
}

async fn persist<T: Serialize>(path: Option<&Path>, receipt: &T) -> Result<(), CliError> {
    match path {
        Some(path) => recovery::replace(path, receipt).await,
        None => Ok(()),
    }
}

#[cfg(unix)]
fn same_physical_file(first: &std::fs::Metadata, second: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    first.dev() == second.dev() && first.ino() == second.ino()
}

#[cfg(not(unix))]
fn same_physical_file(_first: &std::fs::Metadata, _second: &std::fs::Metadata) -> bool {
    false
}

trait IdentityReceipt {
    fn request_seed(&self) -> &str;
    fn correlation_seed(&self) -> &str;
}

struct SourceSnapshot {
    path: PathBuf,
    file: tokio::fs::File,
    byte_size: u64,
    digest: Sha256Digest,
    identity_sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum DownloadPhase {
    OpenPrepared,
    Receiving,
    Verified,
    Published,
    Reported,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DownloadReceipt {
    version: u32,
    kind: String,
    invocation_sha256: String,
    scope_sha256: String,
    session_id: String,
    transfer_id: String,
    artifact_id: String,
    request_seed: String,
    correlation_seed: String,
    temporary_sha256: String,
    byte_size: Option<u64>,
    content_sha256: Option<String>,
    media_type_sha256: Option<String>,
    verified_offset: u64,
    next_ordinal: u64,
    prefix_sha256: String,
    phase: DownloadPhase,
    output_delivered: bool,
}

impl IdentityReceipt for DownloadReceipt {
    fn request_seed(&self) -> &str {
        &self.request_seed
    }

    fn correlation_seed(&self) -> &str {
        &self.correlation_seed
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum UploadPhase {
    BeginPrepared,
    Uploading,
    CompletionPrepared,
    Settled,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PendingChunk {
    ordinal: u64,
    offset: u64,
    byte_size: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct UploadReceipt {
    version: u32,
    kind: String,
    invocation_sha256: String,
    scope_sha256: String,
    session_id: String,
    transfer_id: String,
    artifact_id: String,
    source_identity_sha256: String,
    byte_size: u64,
    content_sha256: String,
    request_seed: String,
    correlation_seed: String,
    acknowledged_offset: u64,
    next_ordinal: u64,
    pending: Option<PendingChunk>,
    phase: UploadPhase,
    output_delivered: bool,
}

impl IdentityReceipt for UploadReceipt {
    fn request_seed(&self) -> &str {
        &self.request_seed
    }

    fn correlation_seed(&self) -> &str {
        &self.correlation_seed
    }
}
