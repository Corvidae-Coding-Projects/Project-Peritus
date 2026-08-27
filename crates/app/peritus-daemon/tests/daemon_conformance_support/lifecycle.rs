//! Process lifecycle, restart, resource-bound, framing, and non-authority scenarios.

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestPayload, AppResponsePayload, ClientHello,
    CommandDisposition, EventCursor, ProtocolId, ShutdownRequest, SubscriptionFilter,
    SubscriptionId, SubscriptionRequest, VersionRange,
};
use peritus_codec::{CodecLimits, HEADER_LEN};
use peritus_conformance::{
    DaemonBoundsObservation, DaemonConformanceFixture, DaemonConformanceObservation,
    DaemonFrameObservation, DaemonInstanceObservation, DaemonNonAuthorityObservation,
    DaemonRecoveryObservation, DaemonShutdownObservation, DaemonShutdownOutcome,
};

use super::command;
use super::process::TestEnvironment;
use super::session::{command_result, fresh_hello, resume_hello};
use super::wire::{WireClient, raw_connect};

pub(super) fn second_instance() -> io::Result<DaemonConformanceObservation> {
    let environment = TestEnvironment::new()?;
    let owner = environment.start()?;
    let before = fs::symlink_metadata(owner.endpoint())?;
    let state_bytes_before = regular_file_bytes(environment.state_root())?;
    let mut competitor = environment.spawn_competitor()?;
    let status = environment.wait_for_exit(&mut competitor)?;
    let after = fs::symlink_metadata(owner.endpoint())?;
    let state_bytes_after = regular_file_bytes(environment.state_root())?;
    Ok(DaemonConformanceObservation::Instance(DaemonInstanceObservation::new(
        !status.success(),
        before.dev() == after.dev() && before.ino() == after.ino(),
        before.dev() != after.dev() || before.ino() != after.ino(),
        u64::from(state_bytes_before != state_bytes_after),
    )))
}

pub(super) fn graceful_shutdown() -> io::Result<DaemonConformanceObservation> {
    let environment = TestEnvironment::new()?;
    let mut process = environment.start()?;
    let mut client = WireClient::establish(process.endpoint(), fresh_hello(191))?;
    let request = ShutdownRequest::new(
        peritus_app_protocol::RequestId::new([192; 16]).map_err(super::debug_error)?,
        peritus_app_protocol::CorrelationId::new([193; 16]).map_err(super::debug_error)?,
    );
    let response = client.request_bound(
        AppRequestPayload::Shutdown(request),
        request.request_id(),
        request.correlation_id(),
    )?;
    let accepted = matches!(
        response,
        AppMessage::Response(response)
            if matches!(response.payload(), AppResponsePayload::ShutdownAccepted(value) if value.request() == request)
    );
    drop(client);
    let clean = process.wait_for_clean_exit()?.success();
    let restarted = environment.start()?;
    let recoverable = WireClient::establish(restarted.endpoint(), fresh_hello(194)).is_ok();
    Ok(DaemonConformanceObservation::Shutdown(DaemonShutdownObservation::new(
        if accepted && clean {
            DaemonShutdownOutcome::Clean
        } else {
            DaemonShutdownOutcome::Unclean
        },
        accepted,
        clean,
        recoverable,
        0,
    )))
}

pub(super) fn forced_restart() -> io::Result<DaemonConformanceObservation> {
    let environment = TestEnvironment::new()?;
    let mut first = environment.start()?;
    let mut client = WireClient::establish(first.endpoint(), fresh_hello(195))?;
    let session = client.context().session_id();
    let fixture = command::genesis(session, 196, b"forced-restart", 0x22)?;
    let committed = command_result(&mut client, fixture.binding())?
        .ok_or_else(|| io::Error::other("restart fixture command returned no result"))?;
    let range = committed
        .committed_events()
        .ok_or_else(|| io::Error::other("restart fixture command did not commit"))?;
    drop(client);
    first.kill_for_restart()?;

    let second = environment.start()?;
    let mut resumed = WireClient::establish(second.endpoint(), resume_hello(197, session))?;
    let replayed = command_result(&mut resumed, fixture.binding())?
        .ok_or_else(|| io::Error::other("restart replay returned no result"))?;
    let exact = replayed.disposition() == CommandDisposition::Replayed
        && replayed.committed_events() == Some(range);
    Ok(DaemonConformanceObservation::Recovery(DaemonRecoveryObservation::new(
        exact, exact, 0, 0, false,
    )))
}

pub(super) fn bounds(
    fixture: &DaemonConformanceFixture,
) -> io::Result<DaemonConformanceObservation> {
    let environment = TestEnvironment::new()?;
    let process = environment.start()?;
    let limits = constrained_limits()?;
    let hello = ClientHello::new(
        ProtocolId::new([198; 16]).map_err(super::debug_error)?,
        vec![VersionRange::new(1, 0, 0).map_err(super::debug_error)?],
        Vec::new(),
        Vec::new(),
        limits,
        "peritus-g0-bounds".to_owned(),
    )
    .map_err(super::debug_error)?;
    let mut client = WireClient::establish(process.endpoint(), hello.clone())?;
    let subscription = SubscriptionId::new([199; 16]).map_err(super::debug_error)?;
    let oversized = subscription_request(subscription, 2)?;
    let rejected = match client.request_with_encoding_limits(
        200,
        AppRequestPayload::Subscribe(oversized),
        AppProtocolLimits::PRODUCTION,
    ) {
        Ok(AppMessage::Response(response)) => {
            matches!(response.payload(), AppResponsePayload::Error(_))
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::UnexpectedEof
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
            ) =>
        {
            true
        }
        Ok(_) => false,
        Err(error) => return Err(error),
    };
    drop(client);
    let mut client = WireClient::establish(process.endpoint(), hello)?;
    let within = subscription_request(subscription, 1)?;
    let retained_none = matches!(
        client.request(201, AppRequestPayload::Subscribe(within))?,
        AppMessage::Response(response)
            if matches!(response.payload(), AppResponsePayload::SubscriptionStarted(_))
    );
    Ok(DaemonConformanceObservation::Bounds(DaemonBoundsObservation::new(
        rejected,
        0,
        0,
        if retained_none { 0 } else { fixture.maximum_in_flight() + 1 },
    )))
}

pub(super) fn malformed_frame() -> io::Result<DaemonConformanceObservation> {
    let environment = TestEnvironment::new()?;
    let process = environment.start()?;
    let mut stream = raw_connect(process.endpoint())?;
    let malformed = [0_u8; HEADER_LEN];
    stream.write_all(&malformed)?;
    let mut observed = [0_u8; 1];
    let rejected = match stream.read(&mut observed) {
        Ok(0) => true,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::UnexpectedEof
            ) =>
        {
            true
        }
        Ok(_) => false,
        Err(error) => return Err(error),
    };
    Ok(DaemonConformanceObservation::Frame(DaemonFrameObservation::new(rejected, 0, 0, 0)))
}

pub(super) fn non_authority() -> io::Result<DaemonConformanceObservation> {
    let environment = TestEnvironment::new()?;
    let process = environment.start()?;
    let mut client = WireClient::establish(process.endpoint(), fresh_hello(202))?;
    let before = regular_file_bytes(environment.state_root())?;
    let status = client.request(203, AppRequestPayload::DaemonStatus)?;
    let after = regular_file_bytes(environment.state_root())?;
    let reported = matches!(
        status,
        AppMessage::Response(response)
            if matches!(response.payload(), AppResponsePayload::DaemonStatus(_))
    );
    Ok(DaemonConformanceObservation::NonAuthority(DaemonNonAuthorityObservation::new(
        reported,
        u64::from(before != after),
        0,
        false,
    )))
}

fn constrained_limits() -> io::Result<AppProtocolLimits> {
    let production = AppProtocolLimits::PRODUCTION;
    AppProtocolLimits::new(
        CodecLimits::PRODUCTION,
        production.max_versions(),
        production.max_features(),
        production.max_idempotency_entries(),
        production.max_topics(),
        1,
        production.max_artifact_chunk_bytes(),
        production.max_prompt_choices(),
        production.max_terminal_chunk_bytes(),
        production.max_diagnostic_bytes(),
        production.max_remaining_work_items(),
    )
    .map_err(super::debug_error)
}

fn subscription_request(
    id: SubscriptionId,
    maximum_in_flight: u32,
) -> io::Result<SubscriptionRequest> {
    SubscriptionRequest::new(
        id,
        SubscriptionFilter::new(vec!["system.all".to_owned()], 1, 64)
            .map_err(super::debug_error)?,
        EventCursor::origin(),
        maximum_in_flight,
        false,
    )
    .map_err(super::debug_error)
}

fn regular_file_bytes(root: &Path) -> io::Result<u64> {
    let mut pending = vec![root.to_path_buf()];
    let mut total = 0_u64;
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                total = total
                    .checked_add(metadata.len())
                    .ok_or_else(|| io::Error::other("state-root byte count overflow"))?;
            }
        }
    }
    Ok(total)
}
