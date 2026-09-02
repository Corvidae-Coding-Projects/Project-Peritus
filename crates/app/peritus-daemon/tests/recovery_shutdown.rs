//! Black-box durable restart, command replay, and authenticated shutdown tests.

#![cfg(unix)]
#![allow(
    clippy::future_not_send,
    reason = "the recovery fixture intentionally owns its SQLite-backed daemon on one current-thread runtime"
)]

mod support;

use std::{future::Future, time::Duration};

use peritus_app_protocol::{
    AppEventPayload, AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload,
    AppResponsePayload, ClientHello, CommandBinding, CommandDisposition, CommandSubmissionFrames,
    CorrelationId, IdempotencyKey, NegotiationOutcome, ProtocolContext, ProtocolId, RequestId,
    ShutdownComplete, ShutdownCompletionDisposition, ShutdownRequest, VersionRange,
};
use peritus_codec::{CodecLimits, encode_message};
use peritus_daemon::{AppFrameStream, DaemonRuntime, LocalEndpointAddress};
use peritus_kernel::CommandEnvelope;
use peritus_protocol::CommandEnvelopeDto;
use peritus_scheduler::{
    ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerBinding,
    SchedulerCommand, SchedulerCommandFrame, SchedulerCommandKind, SchedulerId, SchedulerLimits,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, SessionId, Sha256Digest, WorkspaceId,
};
use tokio::{net::UnixStream, runtime::Builder};

const IO_BOUND: Duration = Duration::from_secs(5);
const LIFECYCLE_BOUND: Duration = Duration::from_secs(10);

#[test]
fn authenticated_shutdown_retains_the_exact_request_and_finishes_cleanly() {
    run_async_test(authenticated_shutdown_retains_the_exact_request_and_finishes_cleanly_async());
}

async fn authenticated_shutdown_retains_the_exact_request_and_finishes_cleanly_async() {
    let temporary = support::temporary_root();
    let mut runtime = tokio::time::timeout(
        LIFECYCLE_BOUND,
        DaemonRuntime::start(support::configuration(temporary.path())),
    )
    .await
    .expect("daemon startup completes within the bound")
    .expect("daemon starts");
    let (mut frames, context) = connect_and_establish(&runtime, fresh_hello(3)).await;
    let shutdown = ShutdownRequest::new(
        RequestId::new([41; 16]).expect("shutdown request identity"),
        CorrelationId::new([42; 16]).expect("shutdown correlation identity"),
    );
    let request = AppRequestEnvelope::new(
        context,
        shutdown.request_id(),
        shutdown.correlation_id(),
        AppRequestPayload::Shutdown(shutdown),
    )
    .expect("correlated shutdown envelope");

    write_frame(&mut frames, &AppMessage::Request(request)).await;
    let AppMessage::Response(response) = read_frame(&mut frames).await else {
        panic!("shutdown did not return a response");
    };
    let AppResponsePayload::ShutdownAccepted(accepted) = response.payload() else {
        panic!("shutdown was not accepted: {:?}", response.payload());
    };
    assert_eq!(accepted.request(), shutdown);

    tokio::time::timeout(IO_BOUND, runtime.wait_for_shutdown_signal())
        .await
        .expect("daemon observes the authenticated shutdown within the bound")
        .expect("shutdown request remains observable");
    assert_eq!(runtime.accepted_shutdown_request(), Some(shutdown));
    let (outcome, (progress_count, wire_complete)) = tokio::time::timeout(
        LIFECYCLE_BOUND,
        futures_util::future::join(
            runtime.shutdown(),
            read_shutdown_completion(&mut frames, shutdown),
        ),
    )
    .await
    .expect("shutdown completes within the configured test bound");
    let outcome = outcome.expect("shutdown coordinator completes");
    assert_eq!(progress_count, 6);
    assert_eq!(wire_complete.request(), shutdown);
    assert_eq!(wire_complete.disposition(), ShutdownCompletionDisposition::Clean);
    assert!(wire_complete.remaining().is_empty());
    assert_eq!(outcome.disposition(), ShutdownCompletionDisposition::Clean);
    assert!(outcome.remaining().is_empty());
    assert!(outcome.failures().is_empty());
    let complete = outcome.protocol().expect("client shutdown has correlated completion");
    assert_eq!(complete.request(), shutdown);
    assert_eq!(complete.disposition(), ShutdownCompletionDisposition::Clean);
    assert!(complete.remaining().is_empty());
}

async fn read_shutdown_completion(
    frames: &mut AppFrameStream<UnixStream>,
    shutdown: ShutdownRequest,
) -> (usize, ShutdownComplete) {
    let mut progress_count = 0;
    loop {
        let AppMessage::Event(event) = read_frame(frames).await else {
            panic!("shutdown stream returned a non-event frame");
        };
        match event.payload() {
            AppEventPayload::ShutdownProgress(progress) => {
                assert_eq!(progress.request(), shutdown);
                progress_count += 1;
            }
            AppEventPayload::ShutdownComplete(complete) => {
                return (progress_count, complete.clone());
            }
            payload => panic!("shutdown stream returned an unrelated event: {payload:?}"),
        }
    }
}

#[test]
fn restart_resumes_the_durable_session_and_replays_the_exact_command_result() {
    run_async_test(restart_resumes_the_durable_session_and_replays_the_exact_command_result_async());
}

async fn restart_resumes_the_durable_session_and_replays_the_exact_command_result_async() {
    let temporary = support::temporary_root();
    let config = support::configuration(temporary.path());
    let first = tokio::time::timeout(LIFECYCLE_BOUND, DaemonRuntime::start(config.clone()))
        .await
        .expect("first startup completes within the bound")
        .expect("first daemon starts");
    let (mut first_frames, first_context) = connect_and_establish(&first, fresh_hello(51)).await;
    let session = first_context.session_id();
    let binding = scheduler_command_binding(session, 52, b"restart-stable-scheduler-genesis");

    let first_request = AppRequestEnvelope::new(
        first_context,
        binding.request_id(),
        binding.correlation_id(),
        AppRequestPayload::SubmitCommand(binding.clone()),
    )
    .expect("initial scheduler request");
    write_frame(&mut first_frames, &AppMessage::Request(first_request)).await;
    let AppMessage::Response(first_response) = read_frame(&mut first_frames).await else {
        panic!("initial scheduler command did not return a response");
    };
    let AppResponsePayload::CommandResult(first_result) = first_response.payload() else {
        panic!("initial scheduler command returned {:?}", first_response.payload());
    };
    assert_eq!(first_result.disposition(), CommandDisposition::Committed);
    let committed = first_result.committed_events().expect("initial committed event range");
    assert_eq!(committed.count(), 1);
    drop(first_frames);
    let first_outcome = tokio::time::timeout(LIFECYCLE_BOUND, first.shutdown())
        .await
        .expect("first daemon shuts down within the bound")
        .expect("first daemon shuts down");
    assert_eq!(first_outcome.disposition(), ShutdownCompletionDisposition::Clean);

    let second = tokio::time::timeout(LIFECYCLE_BOUND, DaemonRuntime::start(config))
        .await
        .expect("restart completes within the bound")
        .expect("daemon restarts from durable state");
    let (mut second_frames, resumed_context) =
        connect_and_establish(&second, resume_hello(61, session)).await;
    assert_eq!(resumed_context.session_id(), session);
    let replay_request = AppRequestEnvelope::new(
        resumed_context,
        binding.request_id(),
        binding.correlation_id(),
        AppRequestPayload::SubmitCommand(binding),
    )
    .expect("replayed scheduler request");
    write_frame(&mut second_frames, &AppMessage::Request(replay_request)).await;
    let AppMessage::Response(replay_response) = read_frame(&mut second_frames).await else {
        panic!("replayed scheduler command did not return a response");
    };
    let AppResponsePayload::CommandResult(replayed) = replay_response.payload() else {
        panic!("replayed scheduler command returned {:?}", replay_response.payload());
    };
    assert_eq!(replayed.disposition(), CommandDisposition::Replayed);
    assert_eq!(replayed.original_request_id(), RequestId::new([52; 16]).expect("request"));
    assert_eq!(replayed.committed_events(), Some(committed));
    drop(second_frames);

    let second_outcome = tokio::time::timeout(LIFECYCLE_BOUND, second.shutdown())
        .await
        .expect("restarted daemon shuts down within the bound")
        .expect("restarted daemon shuts down");
    assert_eq!(second_outcome.disposition(), ShutdownCompletionDisposition::Clean);
}

fn run_async_test(test: impl Future<Output = ()>) {
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build current-thread test runtime");
    runtime.block_on(test);
}

fn fresh_hello(identity: u8) -> ClientHello {
    ClientHello::new(
        ProtocolId::new([identity; 16]).expect("protocol identity"),
        vec![VersionRange::new(1, 0, 0).expect("version")],
        Vec::new(),
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "peritus-recovery-test".to_owned(),
    )
    .expect("client hello")
}

fn resume_hello(identity: u8, session: SessionId) -> ClientHello {
    ClientHello::new_with_session(
        ProtocolId::new([identity; 16]).expect("protocol identity"),
        Some(session),
        vec![VersionRange::new(1, 0, 0).expect("version")],
        Vec::new(),
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "peritus-recovery-resume-test".to_owned(),
    )
    .expect("resume hello")
}

async fn connect_and_establish(
    runtime: &DaemonRuntime,
    client: ClientHello,
) -> (AppFrameStream<UnixStream>, ProtocolContext) {
    let LocalEndpointAddress::Unix(socket) = runtime.endpoint_address().clone();
    let stream = tokio::time::timeout(IO_BOUND, UnixStream::connect(&socket))
        .await
        .expect("socket connection completes within the bound")
        .expect("connect protected socket");
    let mut frames = AppFrameStream::new(stream, AppProtocolLimits::PRODUCTION);
    write_frame(&mut frames, &AppMessage::ClientHello(client.clone())).await;
    let AppMessage::ServerHello(server) = read_frame(&mut frames).await else {
        panic!("daemon did not answer with ServerHello");
    };
    let negotiated = match server.outcome() {
        NegotiationOutcome::Compatible(value) | NegotiationOutcome::Downgraded(value) => value,
        NegotiationOutcome::Incompatible(reason) => {
            panic!("unexpected incompatibility: {reason:?}")
        }
    };
    let session = server.established_session().expect("durable session");
    let context = ProtocolContext::new(client.protocol_id(), negotiated.version(), session);
    (frames, context)
}

async fn write_frame(frames: &mut AppFrameStream<UnixStream>, message: &AppMessage) {
    tokio::time::timeout(IO_BOUND, frames.write(message))
        .await
        .expect("frame write completes within the bound")
        .expect("write application frame");
}

async fn read_frame(frames: &mut AppFrameStream<UnixStream>) -> AppMessage {
    tokio::time::timeout(IO_BOUND, frames.read())
        .await
        .expect("frame read completes within the bound")
        .expect("read application frame")
}

fn scheduler_command_binding(
    session: SessionId,
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
