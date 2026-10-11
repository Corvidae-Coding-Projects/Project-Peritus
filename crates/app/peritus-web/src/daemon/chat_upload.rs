//! Session-scoped transfers of immutable message and attachment snapshots.
use super::{App, Client, PreparedChat, Result, problem};
use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, ArtifactChunk, ArtifactCompletion, ArtifactMetadata,
    CanonicalMediaType, TransferId, WellKnownProtocolFeature, WorkbenchFileImportPreview,
    WorkbenchFileImportRequest, WorkbenchFileMetadata, WorkbenchFileMode, WorkbenchFileRange,
    WorkbenchFileRequest, WorkbenchFileUpload, WorkbenchInputText, WorkbenchIntent,
};
use peritus_types::{ArtifactId, Sha256Digest};
use sha2::{Digest as _, Sha256};
use std::io::Read;

pub(super) async fn prepare(
    app: &App,
    prepared: &PreparedChat,
    revision: u64,
) -> Result<WorkbenchIntent> {
    let endpoint = super::endpoint(app)?;
    let mut client = Client::connect(
        endpoint.as_os_str(),
        None,
        std::time::Duration::from_secs(30),
        &[
            WellKnownProtocolFeature::WorkbenchFiles,
            WellKnownProtocolFeature::WorkbenchReferencedText,
        ],
    )
    .await
    .map_err(problem)?;
    let message = if prepared.text.len() > peritus_app_protocol::MAX_WORKBENCH_INPUT_BYTES {
        let digest = Sha256Digest::new(Sha256::digest(prepared.text.as_bytes()).into());
        Some(
            upload(
                &mut client,
                prepared,
                revision,
                "User message",
                prepared.text.len() as u64,
                digest,
                &mut prepared.text.as_bytes(),
            )
            .await?,
        )
    } else {
        None
    };
    let mut attachments = Vec::new();
    for attachment in &prepared.attachments {
        let mut source = crate::files::attachments::open(app, attachment)?;
        let digest = parse_digest(&attachment.digest)?;
        attachments.push(
            upload(
                &mut client,
                prepared,
                revision,
                &attachment.path,
                attachment.bytes,
                digest,
                &mut source,
            )
            .await?,
        );
    }
    let text = if message.is_some() {
        "Read the complete user_message reference with attachment_read, following all continuation offsets before acting; it contains the user's exact instructions."
    } else {
        &prepared.text
    };
    Ok(WorkbenchIntent::EnqueueMessageBundle {
        text: WorkbenchInputText::new(text.to_owned()).map_err(problem)?,
        message,
        attachments,
    })
}

fn parse_digest(value: &str) -> Result<Sha256Digest> {
    if value.len() != 64 {
        return Err(problem("Invalid retained snapshot digest"));
    }
    let mut digest = [0; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(problem)?;
    }
    Ok(Sha256Digest::new(digest))
}

async fn exchange(client: &mut Client, payload: AppRequestPayload) -> Result<AppResponsePayload> {
    let identity = Client::new_request_identity().map_err(problem)?;
    let response = client.request(identity, payload).await.map_err(problem)?;
    match response.payload() {
        AppResponsePayload::Error(error) => {
            Err(problem(format!("Snapshot transfer rejected: {error}")))
        }
        value => Ok(value.clone()),
    }
}

async fn upload(
    client: &mut Client,
    prepared: &PreparedChat,
    revision: u64,
    label: &str,
    length: u64,
    digest: Sha256Digest,
    source: &mut impl Read,
) -> Result<WorkbenchFileImportPreview> {
    let artifact = ArtifactId::new(super::bytes(&crate::state::id()?)?)
        .map_err(|error| problem(format!("{error:?}")))?;
    let transfer = TransferId::new(super::bytes(&crate::state::id()?)?)
        .map_err(|error| problem(format!("{error:?}")))?;
    let maximum = client.limits().max_artifact_chunk_bytes();
    let chunk_size = maximum.min(64 * 1024);
    let metadata = ArtifactMetadata::new(
        transfer,
        artifact,
        length,
        CanonicalMediaType::new("text/plain".to_owned(), 128).map_err(problem)?,
        digest,
        u32::try_from(chunk_size).map_err(problem)?,
        maximum,
    )
    .map_err(problem)?;
    exchange(
        client,
        AppRequestPayload::BeginWorkbenchFileUpload(
            WorkbenchFileUpload::new(prepared.query, revision, metadata).map_err(problem)?,
        ),
    )
    .await?;
    let mut buffer = vec![0; chunk_size];
    let mut offset = 0_u64;
    let mut index = 0_u64;
    let mut observed = Sha256::new();
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        observed.update(&buffer[..count]);
        exchange(
            client,
            AppRequestPayload::UploadArtifactChunk(
                ArtifactChunk::new(
                    transfer,
                    artifact,
                    index,
                    offset,
                    buffer[..count].to_vec(),
                    maximum,
                )
                .map_err(problem)?,
            ),
        )
        .await?;
        offset += count as u64;
        index += 1;
    }
    if offset != length || observed.finalize().as_slice() != digest.as_bytes() {
        return Err(problem("The retained attachment snapshot changed; no message was admitted."));
    }
    exchange(
        client,
        AppRequestPayload::CompleteArtifactUpload(ArtifactCompletion::new(
            transfer, artifact, length, digest,
        )),
    )
    .await?;
    let selection = WorkbenchFileRequest::new(
        prepared.query,
        revision,
        label.to_owned(),
        WorkbenchFileRange::All,
        WorkbenchFileMode::Snapshot,
        prepared.providers.writer(),
        prepared.models.writer().clone(),
    )
    .map_err(problem)?;
    let file = WorkbenchFileMetadata::new(digest, length, (0, length), digest).map_err(problem)?;
    let request = WorkbenchFileImportRequest::new(selection, artifact, file).map_err(problem)?;
    match exchange(client, AppRequestPayload::PreviewWorkbenchFileImport(request.clone())).await? {
        AppResponsePayload::WorkbenchFileImportPreview(preview)
            if preview.request() == &request =>
        {
            Ok(preview)
        }
        _ => Err(problem("Unexpected snapshot preview response")),
    }
}
