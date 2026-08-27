//! Immutable artifact upload, download, and corruption scenarios.

use std::io;

use peritus_app_protocol::{
    AppEventPayload, AppMessage, AppRequestPayload, AppResponsePayload, ArtifactChunk,
    ArtifactCompletion, ArtifactMetadata, ArtifactOpenRequest, CanonicalMediaType, TransferId,
};
use peritus_conformance::{
    DaemonArtifactIntegrity, DaemonArtifactObservation, DaemonArtifactOutcome,
    DaemonArtifactPublication, DaemonConformanceFixture, DaemonConformanceObservation,
};
use peritus_types::ArtifactId;

use super::process::TestEnvironment;
use super::session::fresh_hello;
use super::wire::WireClient;

pub(super) fn download(
    fixture: &DaemonConformanceFixture,
) -> io::Result<DaemonConformanceObservation> {
    let (_environment, mut process, mut client) = established(71)?;
    let prepared = upload_exact(&mut client, fixture, 72)?;
    let transfer = TransferId::new([75; 16]).map_err(super::debug_error)?;
    let message = client.request(
        76,
        AppRequestPayload::OpenArtifact(ArtifactOpenRequest::new(transfer, prepared.artifact)),
    )?;
    let AppMessage::Response(response) = message else {
        return Err(io::Error::other("artifact open returned a non-response message"));
    };
    let AppResponsePayload::ArtifactOpened(metadata) = response.payload() else {
        return Err(io::Error::other("artifact open was not accepted"));
    };
    let identity_exact = metadata.artifact_id() == prepared.artifact
        && metadata.transfer_id() == transfer
        && metadata.byte_size() == fixture.artifact_size()
        && metadata.digest() == prepared.digest;
    let mut next_offset = 0_u64;
    let mut downloaded = Vec::new();
    loop {
        let AppMessage::Event(event) = client.read().map_err(|error| {
            io::Error::other(format!("artifact download read: {error}: {}", process.diagnostic()))
        })?
        else {
            return Err(io::Error::other("artifact download emitted a non-event message"));
        };
        match event.payload() {
            AppEventPayload::ArtifactChunk(chunk) => {
                if chunk.transfer_id() != transfer || chunk.offset() != next_offset {
                    return Err(io::Error::other("artifact chunk identity or offset drifted"));
                }
                next_offset = next_offset
                    .checked_add(u64::try_from(chunk.bytes().len()).map_err(super::debug_error)?)
                    .ok_or_else(|| io::Error::other("artifact download offset overflow"))?;
                downloaded.extend_from_slice(chunk.bytes());
            }
            AppEventPayload::ArtifactComplete(completion) => {
                let exact = identity_exact
                    && completion.transfer_id() == transfer
                    && completion.artifact_id() == prepared.artifact
                    && completion.byte_size() == fixture.artifact_size()
                    && completion.digest() == prepared.digest
                    && downloaded == prepared.bytes;
                return Ok(observation(
                    DaemonArtifactOutcome::Downloaded,
                    u64::try_from(downloaded.len()).map_err(super::debug_error)?,
                    if exact {
                        DaemonArtifactIntegrity::Exact
                    } else {
                        DaemonArtifactIntegrity::Mismatched
                    },
                    DaemonArtifactPublication::Available,
                ));
            }
            _ => {}
        }
    }
}

pub(super) fn upload(
    fixture: &DaemonConformanceFixture,
) -> io::Result<DaemonConformanceObservation> {
    let (_environment, _process, mut client) = established(77)?;
    let prepared = upload_exact(&mut client, fixture, 78)?;
    let opened = client.request(
        81,
        AppRequestPayload::OpenArtifact(ArtifactOpenRequest::new(
            TransferId::new([82; 16]).map_err(super::debug_error)?,
            prepared.artifact,
        )),
    )?;
    let published = matches!(
        opened,
        AppMessage::Response(response)
            if matches!(response.payload(), AppResponsePayload::ArtifactOpened(metadata)
                if metadata.digest() == prepared.digest && metadata.byte_size() == fixture.artifact_size())
    );
    Ok(observation(
        DaemonArtifactOutcome::Uploaded,
        fixture.artifact_size(),
        DaemonArtifactIntegrity::Exact,
        if published {
            DaemonArtifactPublication::Published
        } else {
            DaemonArtifactPublication::Withheld
        },
    ))
}

pub(super) fn corruption(
    fixture: &DaemonConformanceFixture,
) -> io::Result<DaemonConformanceObservation> {
    let (_environment, _process, mut client) = established(83)?;
    let bytes = vec![0x5a; usize::try_from(fixture.artifact_size()).map_err(super::debug_error)?];
    let digest = peritus_codec::sha256(&bytes);
    let bad_digest = peritus_types::Sha256Digest::new([0x7f; 32]);
    let transfer = TransferId::new([84; 16]).map_err(super::debug_error)?;
    let artifact = ArtifactId::new([85; 16]).map_err(super::debug_error)?;
    acknowledge(
        &mut client,
        86,
        AppRequestPayload::BeginArtifactUpload(metadata(
            transfer,
            artifact,
            fixture.artifact_size(),
            digest,
        )?),
    )?;
    acknowledge(
        &mut client,
        87,
        AppRequestPayload::UploadArtifactChunk(
            ArtifactChunk::new(
                transfer,
                artifact,
                0,
                0,
                bytes,
                peritus_app_protocol::AppProtocolLimits::PRODUCTION.max_artifact_chunk_bytes(),
            )
            .map_err(super::debug_error)?,
        ),
    )?;
    let completion = client.request(
        88,
        AppRequestPayload::CompleteArtifactUpload(ArtifactCompletion::new(
            transfer,
            artifact,
            fixture.artifact_size(),
            bad_digest,
        )),
    )?;
    let rejected = matches!(
        completion,
        AppMessage::Response(response)
            if matches!(response.payload(), AppResponsePayload::Error(_))
    );
    let opened = client.request(
        89,
        AppRequestPayload::OpenArtifact(ArtifactOpenRequest::new(
            TransferId::new([90; 16]).map_err(super::debug_error)?,
            artifact,
        )),
    )?;
    let withheld = matches!(
        opened,
        AppMessage::Response(response)
            if matches!(response.payload(), AppResponsePayload::Error(_))
    );
    Ok(observation(
        if rejected {
            DaemonArtifactOutcome::CorruptRejected
        } else {
            DaemonArtifactOutcome::Uploaded
        },
        fixture.artifact_size(),
        DaemonArtifactIntegrity::Mismatched,
        if withheld {
            DaemonArtifactPublication::Withheld
        } else {
            DaemonArtifactPublication::Partial
        },
    ))
}

struct PreparedArtifact {
    artifact: ArtifactId,
    digest: peritus_types::Sha256Digest,
    bytes: Vec<u8>,
}

fn upload_exact(
    client: &mut WireClient,
    fixture: &DaemonConformanceFixture,
    seed: u8,
) -> io::Result<PreparedArtifact> {
    let bytes = (0..fixture.artifact_size())
        .map(|offset| u8::try_from(offset % 251).expect("bounded byte"))
        .collect::<Vec<_>>();
    let digest = peritus_codec::sha256(&bytes);
    let transfer = TransferId::new([seed; 16]).map_err(super::debug_error)?;
    let artifact = ArtifactId::new([seed.wrapping_add(1); 16]).map_err(super::debug_error)?;
    acknowledge(
        client,
        seed.wrapping_add(2),
        AppRequestPayload::BeginArtifactUpload(metadata(
            transfer,
            artifact,
            fixture.artifact_size(),
            digest,
        )?),
    )?;
    let chunk_limit =
        peritus_app_protocol::AppProtocolLimits::PRODUCTION.max_artifact_chunk_bytes();
    acknowledge(
        client,
        seed.wrapping_add(3),
        AppRequestPayload::UploadArtifactChunk(
            ArtifactChunk::new(transfer, artifact, 0, 0, bytes.clone(), chunk_limit)
                .map_err(super::debug_error)?,
        ),
    )?;
    acknowledge(
        client,
        seed.wrapping_add(4),
        AppRequestPayload::CompleteArtifactUpload(ArtifactCompletion::new(
            transfer,
            artifact,
            fixture.artifact_size(),
            digest,
        )),
    )?;
    Ok(PreparedArtifact { artifact, digest, bytes })
}

fn metadata(
    transfer: TransferId,
    artifact: ArtifactId,
    bytes: u64,
    digest: peritus_types::Sha256Digest,
) -> io::Result<ArtifactMetadata> {
    ArtifactMetadata::new(
        transfer,
        artifact,
        bytes,
        CanonicalMediaType::new("application/octet-stream".to_owned(), 255)
            .map_err(super::debug_error)?,
        digest,
        1,
        peritus_app_protocol::AppProtocolLimits::PRODUCTION.max_artifact_chunk_bytes(),
    )
    .map_err(super::debug_error)
}

fn acknowledge(client: &mut WireClient, id: u8, payload: AppRequestPayload) -> io::Result<()> {
    let message = client.request(id, payload)?;
    if matches!(
        message,
        AppMessage::Response(response)
            if matches!(response.payload(), AppResponsePayload::Acknowledged(_))
    ) {
        Ok(())
    } else {
        Err(io::Error::other("artifact operation was not acknowledged"))
    }
}

fn established(
    seed: u8,
) -> io::Result<(TestEnvironment, super::process::DaemonProcess, WireClient)> {
    let environment = TestEnvironment::new()?;
    let process = environment.start()?;
    let client = WireClient::establish(process.endpoint(), fresh_hello(seed))?;
    Ok((environment, process, client))
}

const fn observation(
    outcome: DaemonArtifactOutcome,
    bytes: u64,
    integrity: DaemonArtifactIntegrity,
    publication: DaemonArtifactPublication,
) -> DaemonConformanceObservation {
    DaemonConformanceObservation::Artifact(DaemonArtifactObservation::new(
        outcome,
        bytes,
        integrity,
        publication,
    ))
}
