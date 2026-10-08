//! One owned scoped upload completes before any queue or external-context admission.

use super::{App, PreparedChat, Result, problem};
use crate::daemon::{self, Client};
use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, ArtifactChunk, ArtifactCompletion, ArtifactMetadata,
    CanonicalMediaType, TransferId, WellKnownProtocolFeature, WorkbenchFileUpload,
};
use peritus_types::{ArtifactId, Sha256Digest};
use tokio::io::AsyncReadExt as _;

pub(super) enum Body<'a> { Memory(&'a [u8]), File(tokio::fs::File) }

impl Body<'_> {
    async fn next(&mut self, maximum: usize) -> Result<Vec<u8>> {
        match self {
            Self::Memory(bytes) => {
                let count = bytes.len().min(maximum);
                let chunk = bytes[..count].to_vec();
                *bytes = &bytes[count..];
                Ok(chunk)
            }
            Self::File(file) => {
                let mut chunk = vec![0; maximum];
                let count = file.read(&mut chunk).await?;
                chunk.truncate(count);
                Ok(chunk)
            }
        }
    }
}

pub(super) async fn complete(
    app: &App, prepared: &PreparedChat, revision: u64, artifact: ArtifactId,
    digest: Sha256Digest, bytes: u64, feature: WellKnownProtocolFeature, mut body: Body<'_>,
) -> Result<()> {
    let mut client = daemon::connect_owned(app, prepared.owner()?, &[
        WellKnownProtocolFeature::WorkbenchFiles, feature,
    ]).await?;
    let maximum = client.limits().max_artifact_chunk_bytes().min(64 * 1024);
    let transfer = TransferId::new(daemon::bytes(&crate::state::id()?)?)
        .map_err(|error| problem(format!("{error:?}")))?;
    let metadata = ArtifactMetadata::new(
        transfer, artifact, bytes, CanonicalMediaType::new("text/plain".into(), 255).map_err(problem)?,
        digest, u32::try_from(maximum).map_err(problem)?, maximum,
    ).map_err(problem)?;
    acknowledge(&mut client, AppRequestPayload::BeginWorkbenchFileUpload(
        WorkbenchFileUpload::new(prepared.query, revision, metadata).map_err(problem)?,
    )).await?;
    let mut offset = 0_u64;
    let mut ordinal = 0_u64;
    loop {
        let chunk = body.next(maximum).await?;
        if chunk.is_empty() { break; }
        let length = u64::try_from(chunk.len()).map_err(problem)?;
        let end = offset.checked_add(length).ok_or_else(|| problem("Attachment byte offset overflowed"))?;
        if end > bytes { return Err(problem("The attachment grew after snapshot selection")); }
        acknowledge(&mut client, AppRequestPayload::UploadArtifactChunk(
            ArtifactChunk::new(transfer, artifact, ordinal, offset, chunk, maximum).map_err(problem)?,
        )).await?;
        offset = end;
        ordinal = ordinal.checked_add(1).ok_or_else(|| problem("Attachment chunk ordinal overflowed"))?;
    }
    if offset != bytes { return Err(problem("The attachment length changed after snapshot selection")); }
    acknowledge(&mut client, AppRequestPayload::CompleteArtifactUpload(
        ArtifactCompletion::new(transfer, artifact, bytes, digest),
    )).await
}

async fn acknowledge(client: &mut Client, payload: AppRequestPayload) -> Result<()> {
    let identity = Client::new_request_identity().map_err(problem)?;
    let response = client.request(identity, payload).await.map_err(problem)?;
    match response.payload() {
        AppResponsePayload::Acknowledged(acknowledgment) if acknowledgment.request_id() == identity.request_id => Ok(()),
        AppResponsePayload::Error(error) => Err(problem(format!("The daemon rejected the exact snapshot upload: {error}"))),
        _ => Err(problem("The daemon did not acknowledge the exact snapshot transfer")),
    }
}

pub(super) fn digest(value: &str) -> Result<Sha256Digest> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')) {
        return Err(problem("The retained attachment digest is not canonical SHA-256"));
    }
    let mut bytes = [0; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(problem)?;
    }
    Ok(Sha256Digest::new(bytes))
}
