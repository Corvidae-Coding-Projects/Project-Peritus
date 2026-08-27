//! Black-box production daemon startup and authenticated local protocol tests.

#![cfg(unix)]

use std::path::Path;

use peritus_app_protocol::{
    AppEventPayload, AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload,
    AppResponsePayload, ArtifactChunk, ArtifactCompletion, ArtifactMetadata, ArtifactOpenRequest,
    CanonicalMediaType, ClientHello, CommandBinding, CommandDisposition, CommandSubmissionFrames,
    CorrelationId, IdempotencyKey, NegotiationOutcome, ProtocolContext, ProtocolId, RequestId,
    TransferId, VersionRange,
};
use peritus_codec::{CodecLimits, encode_message};
use peritus_daemon::{AppFrameStream, DaemonConfig, DaemonRuntime, LocalEndpointAddress};
use peritus_kernel::CommandEnvelope;
use peritus_protocol::CommandEnvelopeDto;
use peritus_scheduler::{
    ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerBinding,
    SchedulerCommand, SchedulerCommandFrame, SchedulerCommandKind, SchedulerId, SchedulerLimits,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};
use tempfile::TempDir;
use tokio::net::UnixStream;

fn configuration(root: &Path) -> DaemonConfig {
    let state = root.join("state");
    let artifacts = state.join("artifacts");
    let evidence = state.join("evidence");
    let workspaces = state.join("workspaces");
    let processes = state.join("processes");
    let transactions = state.join("transactions");
    let backups = state.join("backups");
    let text = format!(
        r#"version = 1
store_id = "11111111111111111111111111111111"

[paths]
state_root = "{}"
artifact_root = "{}"
evidence_root = "{}"
workspace_root = "{}"
process_root = "{}"
transaction_root = "{}"
backup_root = "{}"

[human]
actor_id = "22222222222222222222222222222222"

[telemetry]
mode = "disabled"
"#,
        state.display(),
        artifacts.display(),
        evidence.display(),
        workspaces.display(),
        processes.display(),
        transactions.display(),
        backups.display(),
    );
    DaemonConfig::parse(&text).expect("valid strict daemon configuration")
}

fn client_hello() -> ClientHello {
    ClientHello::new(
        ProtocolId::new([3; 16]).expect("protocol identity"),
        vec![VersionRange::new(1, 0, 0).expect("version")],
        Vec::new(),
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "peritus-daemon-test".to_owned(),
    )
    .expect("client hello")
}

fn resume_hello(session: peritus_types::SessionId) -> ClientHello {
    ClientHello::new_with_session(
        ProtocolId::new([6; 16]).expect("protocol identity"),
        Some(session),
        vec![VersionRange::new(1, 0, 0).expect("version")],
        Vec::new(),
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "peritus-daemon-resume-test".to_owned(),
    )
    .expect("resume hello")
}

#[test]
fn strict_configuration_rejects_unknown_and_plaintext_authority_fields() {
    let temporary = TempDir::new().expect("temporary root");
    let root = temporary.path();
    let valid = configuration(root);
    assert_eq!(valid.version(), 1);
    assert_eq!(valid.human().actor_identity().expect("actor").into_bytes(), [0x22; 16]);

    let invalid = format!(
        r#"version = 1
store_id = "11111111111111111111111111111111"
plaintext_token = "forbidden"
[paths]
state_root = "{0}/state"
artifact_root = "{0}/state/artifacts"
evidence_root = "{0}/state/evidence"
workspace_root = "{0}/state/workspaces"
process_root = "{0}/state/processes"
transaction_root = "{0}/state/transactions"
backup_root = "{0}/state/backups"
[human]
actor_id = "22222222222222222222222222222222"
[telemetry]
mode = "disabled"
"#,
        root.display(),
    );
    assert!(DaemonConfig::parse(&invalid).is_err());
}

#[tokio::test]
async fn runtime_accepts_authenticated_negotiation_and_status() {
    let temporary = TempDir::new().expect("temporary root");
    let runtime =
        DaemonRuntime::start(configuration(temporary.path())).await.expect("daemon starts");
    let LocalEndpointAddress::Unix(socket) = runtime.endpoint_address().clone();
    let stream = UnixStream::connect(socket).await.expect("connect protected socket");
    let mut frames = AppFrameStream::new(stream, AppProtocolLimits::PRODUCTION);
    let client = client_hello();
    frames.write(&AppMessage::ClientHello(client.clone())).await.expect("write hello");
    let AppMessage::ServerHello(server) = frames.read().await.expect("read server hello") else {
        panic!("server did not answer with ServerHello");
    };
    let negotiated = match server.outcome() {
        NegotiationOutcome::Compatible(value) | NegotiationOutcome::Downgraded(value) => value,
        NegotiationOutcome::Incompatible(reason) => {
            panic!("unexpected incompatibility: {reason:?}")
        }
    };
    let session = server.established_session().expect("durable session");
    let context = ProtocolContext::new(client.protocol_id(), negotiated.version(), session);
    let request = AppRequestEnvelope::new(
        context,
        RequestId::new([4; 16]).expect("request"),
        CorrelationId::new([5; 16]).expect("correlation"),
        AppRequestPayload::DaemonStatus,
    )
    .expect("status request");
    frames.write(&AppMessage::Request(request)).await.expect("write status request");
    let AppMessage::Response(response) = frames.read().await.expect("read status response") else {
        panic!("server did not answer with a response");
    };
    let AppResponsePayload::DaemonStatus(status) = response.payload() else {
        panic!("server did not return daemon status");
    };
    assert!(status.mutation_ready());
    drop(frames);

    let LocalEndpointAddress::Unix(socket) = runtime.endpoint_address().clone();
    let stream = UnixStream::connect(socket).await.expect("reconnect protected socket");
    let mut frames = AppFrameStream::new(stream, AppProtocolLimits::PRODUCTION);
    let resumed_client = resume_hello(session);
    frames
        .write(&AppMessage::ClientHello(resumed_client.clone()))
        .await
        .expect("write resume hello");
    let AppMessage::ServerHello(resumed) = frames.read().await.expect("read resumed server hello")
    else {
        panic!("server did not answer resume with ServerHello");
    };
    assert_eq!(resumed.established_session(), Some(session));
    let negotiated = match resumed.outcome() {
        NegotiationOutcome::Compatible(value) | NegotiationOutcome::Downgraded(value) => value,
        NegotiationOutcome::Incompatible(reason) => panic!("resume failed: {reason:?}"),
    };
    let context = ProtocolContext::new(resumed_client.protocol_id(), negotiated.version(), session);
    let wrong_context = ProtocolContext::new(
        ProtocolId::new([7; 16]).expect("wrong protocol identity"),
        negotiated.version(),
        session,
    );
    let request = AppRequestEnvelope::new(
        wrong_context,
        RequestId::new([8; 16]).expect("request"),
        CorrelationId::new([9; 16]).expect("correlation"),
        AppRequestPayload::DaemonStatus,
    )
    .expect("mismatched status request");
    frames.write(&AppMessage::Request(request)).await.expect("write mismatch");
    let AppMessage::Response(response) = frames.read().await.expect("read mismatch response")
    else {
        panic!("server did not reject mismatched context");
    };
    assert!(matches!(
        response.payload(),
        AppResponsePayload::Error(error)
            if error.code() == peritus_app_protocol::AppErrorCode::SessionMismatch
    ));
    assert_ne!(context, wrong_context);
    drop(frames);
    runtime.shutdown().await.expect("clean shutdown");
}

#[tokio::test]
async fn runtime_streams_uploaded_artifacts_through_the_durable_catalog() {
    let temporary = TempDir::new().expect("temporary root");
    let runtime =
        DaemonRuntime::start(configuration(temporary.path())).await.expect("daemon starts");
    let LocalEndpointAddress::Unix(socket) = runtime.endpoint_address().clone();
    let stream = UnixStream::connect(socket).await.expect("connect protected socket");
    let mut frames = AppFrameStream::new(stream, AppProtocolLimits::PRODUCTION);
    let client = client_hello();
    frames.write(&AppMessage::ClientHello(client.clone())).await.expect("write hello");
    let AppMessage::ServerHello(server) = frames.read().await.expect("read server hello") else {
        panic!("server did not answer with ServerHello");
    };
    let negotiated = match server.outcome() {
        NegotiationOutcome::Compatible(value) | NegotiationOutcome::Downgraded(value) => value,
        NegotiationOutcome::Incompatible(reason) => {
            panic!("unexpected incompatibility: {reason:?}")
        }
    };
    let session = server.established_session().expect("durable session");
    let context = ProtocolContext::new(client.protocol_id(), negotiated.version(), session);
    let bytes = b"durable streamed artifact".to_vec();
    let digest = peritus_codec::sha256(&bytes);
    let artifact_id = peritus_types::ArtifactId::new([21; 16]).expect("artifact identity");
    let upload_id = TransferId::new([22; 16]).expect("upload identity");
    let metadata = ArtifactMetadata::new(
        upload_id,
        artifact_id,
        u64::try_from(bytes.len()).expect("artifact size"),
        CanonicalMediaType::new("text/plain".to_owned(), 255).expect("media type"),
        digest,
        64,
        AppProtocolLimits::PRODUCTION.max_artifact_chunk_bytes(),
    )
    .expect("upload metadata");
    request_acknowledged(
        &mut frames,
        context,
        30,
        AppRequestPayload::BeginArtifactUpload(metadata.clone()),
    )
    .await;
    let chunk = ArtifactChunk::new(
        upload_id,
        artifact_id,
        0,
        0,
        bytes.clone(),
        AppProtocolLimits::PRODUCTION.max_artifact_chunk_bytes(),
    )
    .expect("upload chunk");
    request_acknowledged(&mut frames, context, 31, AppRequestPayload::UploadArtifactChunk(chunk))
        .await;
    request_acknowledged(
        &mut frames,
        context,
        32,
        AppRequestPayload::CompleteArtifactUpload(ArtifactCompletion::new(
            upload_id,
            artifact_id,
            u64::try_from(bytes.len()).expect("artifact size"),
            digest,
        )),
    )
    .await;

    let download_id = TransferId::new([23; 16]).expect("download identity");
    let request = request(
        context,
        33,
        AppRequestPayload::OpenArtifact(ArtifactOpenRequest::new(download_id, artifact_id)),
    );
    frames.write(&AppMessage::Request(request)).await.expect("open artifact");
    let AppMessage::Response(response) = frames.read().await.expect("artifact metadata response")
    else {
        panic!("artifact open did not produce a response");
    };
    let AppResponsePayload::ArtifactOpened(opened) = response.payload() else {
        panic!("artifact open was not accepted: {:?}", response.payload());
    };
    assert_eq!(opened.digest(), digest);
    assert_eq!(opened.byte_size(), bytes.len() as u64);

    let AppMessage::Event(chunk_event) = frames.read().await.expect("download chunk") else {
        panic!("download did not emit a chunk event");
    };
    let AppEventPayload::ArtifactChunk(downloaded) = chunk_event.payload() else {
        panic!("download emitted the wrong event: {:?}", chunk_event.payload());
    };
    assert_eq!(downloaded.offset(), 0);
    assert_eq!(downloaded.bytes(), bytes);
    let AppMessage::Event(completion_event) = frames.read().await.expect("download completion")
    else {
        panic!("download did not emit completion");
    };
    let AppEventPayload::ArtifactComplete(completion) = completion_event.payload() else {
        panic!("download emitted the wrong terminal event: {:?}", completion_event.payload());
    };
    assert_eq!(completion.digest(), digest);
    assert_eq!(completion.byte_size(), bytes.len() as u64);
    drop(frames);
    runtime.shutdown().await.expect("clean shutdown");
}

#[tokio::test]
async fn runtime_commits_and_replays_registered_scheduler_commands() {
    let temporary = TempDir::new().expect("temporary root");
    let runtime =
        DaemonRuntime::start(configuration(temporary.path())).await.expect("daemon starts");
    let LocalEndpointAddress::Unix(socket) = runtime.endpoint_address().clone();
    let stream = UnixStream::connect(socket).await.expect("connect protected socket");
    let mut frames = AppFrameStream::new(stream, AppProtocolLimits::PRODUCTION);
    let client = client_hello();
    frames.write(&AppMessage::ClientHello(client.clone())).await.expect("write hello");
    let AppMessage::ServerHello(server) = frames.read().await.expect("read server hello") else {
        panic!("server did not answer with ServerHello");
    };
    let negotiated = match server.outcome() {
        NegotiationOutcome::Compatible(value) | NegotiationOutcome::Downgraded(value) => value,
        NegotiationOutcome::Incompatible(reason) => {
            panic!("unexpected incompatibility: {reason:?}")
        }
    };
    let session = server.established_session().expect("durable session");
    let context = ProtocolContext::new(client.protocol_id(), negotiated.version(), session);
    let binding = scheduler_command_binding(session, 40, b"scheduler-genesis");
    let request = AppRequestEnvelope::new(
        context,
        binding.request_id(),
        binding.correlation_id(),
        AppRequestPayload::SubmitCommand(binding.clone()),
    )
    .expect("scheduler request");
    frames.write(&AppMessage::Request(request)).await.expect("submit scheduler genesis");
    let AppMessage::Response(response) = frames.read().await.expect("scheduler response") else {
        panic!("scheduler command did not return a response");
    };
    let AppResponsePayload::CommandResult(result) = response.payload() else {
        panic!("scheduler command returned the wrong payload: {:?}", response.payload());
    };
    assert_eq!(result.disposition(), CommandDisposition::Committed);
    assert_eq!(result.committed_events().expect("committed range").count(), 1);

    let replay_request = AppRequestEnvelope::new(
        context,
        binding.request_id(),
        binding.correlation_id(),
        AppRequestPayload::SubmitCommand(binding),
    )
    .expect("scheduler replay request");
    frames.write(&AppMessage::Request(replay_request)).await.expect("replay scheduler genesis");
    let AppMessage::Response(response) = frames.read().await.expect("scheduler replay response")
    else {
        panic!("scheduler replay did not return a response");
    };
    let AppResponsePayload::CommandResult(result) = response.payload() else {
        panic!("scheduler replay returned the wrong payload: {:?}", response.payload());
    };
    assert_eq!(result.disposition(), CommandDisposition::Replayed);
    drop(frames);
    runtime.shutdown().await.expect("clean shutdown");
}

fn scheduler_command_binding(
    session: peritus_types::SessionId,
    request_identity: u8,
    key: &[u8],
) -> CommandBinding {
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new([10; 16]).expect("acceptance identity"),
        HarnessId::new([11; 16]).expect("harness identity"),
        WorkspaceId::new([12; 16]).expect("workspace identity"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([13; 16]).expect("policy identity"),
        ProviderProfileId::new([14; 16]).expect("provider identity"),
    );
    let limits = SchedulerLimits::new(128, 512, 16, 16, 8, 16, 4, 2, 8, 1_048_576, 4_194_304)
        .expect("scheduler limits");
    let resources = ResourceVector::new(
        vec![
            ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(8).expect("CPU capacity")),
            ResourceEntry::new(
                ResourceKind::MEMORY_BYTES,
                ResourceQuantity::new(8_192).expect("memory capacity"),
            ),
        ],
        8,
    )
    .expect("resource capacity");
    let run_id = RunId::new([15; 16]).expect("run identity");
    let scheduler = SchedulerBinding::new(
        run_id,
        SchedulerId::new([16; 16]).expect("scheduler identity"),
        revision,
        limits,
        resources,
    )
    .expect("scheduler binding");
    let command_id = CommandId::new([17; 16]).expect("command identity");
    let event_id = EventId::new([18; 16]).expect("event identity");
    let command = SchedulerCommand::new(
        command_id,
        event_id,
        run_id,
        0,
        None,
        Sha256Digest::new([0; 32]),
        revision,
        SchedulerCommandKind::StartScheduler { binding: scheduler },
    )
    .expect("scheduler genesis command");
    let envelope = CommandEnvelope::new(command_id, event_id, None, revision);
    let envelope_bytes =
        encode_message(&CommandEnvelopeDto::from(envelope), CodecLimits::PRODUCTION)
            .expect("command envelope frame");
    let command_bytes =
        encode_message(&SchedulerCommandFrame::from_command(&command), CodecLimits::PRODUCTION)
            .expect("scheduler command frame");
    let submission = CommandSubmissionFrames::parse(
        envelope_bytes,
        command_bytes,
        AppProtocolLimits::PRODUCTION,
    )
    .expect("scheduler submission frames");
    CommandBinding::new(
        ActorId::new([0x22; 16]).expect("configured actor"),
        session,
        RequestId::new([request_identity; 16]).expect("request identity"),
        CorrelationId::new([request_identity.wrapping_add(64); 16]).expect("correlation identity"),
        IdempotencyKey::new(key.to_vec()).expect("idempotency key"),
        Some(revision),
        submission,
    )
    .expect("scheduler application binding")
}

fn request(
    context: ProtocolContext,
    identity: u8,
    payload: AppRequestPayload,
) -> AppRequestEnvelope {
    AppRequestEnvelope::new(
        context,
        RequestId::new([identity; 16]).expect("request identity"),
        CorrelationId::new([identity.wrapping_add(64); 16]).expect("correlation identity"),
        payload,
    )
    .expect("application request")
}

async fn request_acknowledged(
    frames: &mut AppFrameStream<UnixStream>,
    context: ProtocolContext,
    identity: u8,
    payload: AppRequestPayload,
) {
    frames
        .write(&AppMessage::Request(request(context, identity, payload)))
        .await
        .expect("write artifact request");
    let AppMessage::Response(response) = frames.read().await.expect("artifact response") else {
        panic!("artifact operation did not produce a response");
    };
    assert!(
        matches!(response.payload(), AppResponsePayload::Acknowledged(_)),
        "artifact operation failed: {:?}",
        response.payload(),
    );
}
