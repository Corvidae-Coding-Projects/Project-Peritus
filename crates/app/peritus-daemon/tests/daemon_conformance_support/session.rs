//! Session, actor assertion, context, and command black-box scenarios.

use std::io;

use peritus_app_protocol::{
    AppErrorCode, AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload,
    AppResponsePayload, ClientHello, CommandBinding, CommandDisposition, CorrelationId,
    IncompatibilityReason, NegotiationOutcome, ProtocolContext, ProtocolId, RequestId,
    VersionRange,
};
use peritus_conformance::{
    DaemonCommandObservation, DaemonCommandOutcome, DaemonConformanceObservation,
    DaemonSessionObservation, DaemonSessionOutcome,
};
use peritus_types::SessionId;

use super::command;
use super::process::TestEnvironment;
use super::wire::{WireClient, exchange_hello};

mod indeterminate;

pub(super) use indeterminate::indeterminate_command;

pub(super) fn compatible_session() -> io::Result<DaemonConformanceObservation> {
    let environment = TestEnvironment::new()?;
    let process = environment.start()?;
    let first = WireClient::establish(process.endpoint(), fresh_hello(31))?;
    let session = first.context().session_id();
    drop(first);
    let resumed = WireClient::establish(process.endpoint(), resume_hello(32, session))?;
    let stable = resumed.context().session_id() == session;
    Ok(DaemonConformanceObservation::Session(DaemonSessionObservation::new(
        DaemonSessionOutcome::Established,
        stable,
        true,
        true,
        1,
        0,
    )))
}

pub(super) fn incompatible_session() -> io::Result<DaemonConformanceObservation> {
    let environment = TestEnvironment::new()?;
    let process = environment.start()?;
    let (_stream, server) = exchange_hello(process.endpoint(), &incompatible_hello(33))?;
    let incompatible = matches!(
        server.outcome(),
        NegotiationOutcome::Incompatible(IncompatibilityReason::NoCommonVersion)
    );
    Ok(DaemonConformanceObservation::Session(DaemonSessionObservation::new(
        if incompatible {
            DaemonSessionOutcome::Incompatible
        } else {
            DaemonSessionOutcome::Rejected
        },
        server.established_session().is_some(),
        true,
        true,
        0,
        0,
    )))
}

pub(super) fn context_mismatch() -> io::Result<DaemonConformanceObservation> {
    let environment = TestEnvironment::new()?;
    let process = environment.start()?;
    let mut client = WireClient::establish(process.endpoint(), fresh_hello(34))?;
    let context = client.context();
    let wrong = ProtocolContext::new(
        ProtocolId::new([35; 16]).map_err(super::debug_error)?,
        context.version(),
        context.session_id(),
    );
    let request = AppRequestEnvelope::new(
        wrong,
        RequestId::new([36; 16]).map_err(super::debug_error)?,
        CorrelationId::new([37; 16]).map_err(super::debug_error)?,
        AppRequestPayload::DaemonStatus,
    )
    .map_err(super::debug_error)?;
    client.write(&AppMessage::Request(request))?;
    let rejected = matches!(
        client.read()?,
        AppMessage::Response(response)
            if matches!(response.payload(), AppResponsePayload::Error(error) if error.code() == AppErrorCode::SessionMismatch)
    );
    Ok(DaemonConformanceObservation::Session(DaemonSessionObservation::new(
        if rejected { DaemonSessionOutcome::Rejected } else { DaemonSessionOutcome::Established },
        true,
        true,
        false,
        0,
        0,
    )))
}

pub(super) fn peer_actor_mismatch() -> io::Result<DaemonConformanceObservation> {
    let environment = TestEnvironment::new()?;
    let process = environment.start()?;
    let mut client = WireClient::establish(process.endpoint(), fresh_hello(38))?;
    let session = client.context().session_id();
    let wrong = command::genesis(session, 40, b"peer-actor-proof", 0x23)?;
    let rejected = send_command_may_close(&mut client, wrong.binding().clone())?;
    drop(client);

    let mut resumed = WireClient::establish(process.endpoint(), resume_hello(39, session))?;
    let correct = command::genesis(session, 40, b"peer-actor-proof", 0x22)?;
    let committed = command_result(&mut resumed, correct.binding())?
        .is_some_and(|result| result.disposition() == CommandDisposition::Committed);
    Ok(DaemonConformanceObservation::Session(DaemonSessionObservation::new(
        if rejected && committed {
            DaemonSessionOutcome::Rejected
        } else {
            DaemonSessionOutcome::Established
        },
        true,
        false,
        true,
        0,
        0,
    )))
}

pub(super) fn new_command() -> io::Result<DaemonConformanceObservation> {
    let (_environment, _process, mut client) = established(41)?;
    let fixture = command::genesis(client.context().session_id(), 42, b"new-command", 0x22)?;
    let result = command_result(&mut client, fixture.binding())?
        .ok_or_else(|| io::Error::other("new command returned no command result"))?;
    let events =
        result.committed_events().map_or(0, peritus_app_protocol::CommittedEventRange::count);
    Ok(command_observation(
        if result.disposition() == CommandDisposition::Committed {
            DaemonCommandOutcome::Committed
        } else {
            DaemonCommandOutcome::Rejected
        },
        events,
        events > 0,
        false,
        0,
        u64::from(events > 0),
        0,
    ))
}

pub(super) fn replay_command() -> io::Result<DaemonConformanceObservation> {
    let (_environment, _process, mut client) = established(43)?;
    let fixture = command::genesis(client.context().session_id(), 44, b"replay-command", 0x22)?;
    let first = command_result(&mut client, fixture.binding())?
        .ok_or_else(|| io::Error::other("initial command returned no command result"))?;
    let replayed = command_result(&mut client, fixture.binding())?
        .ok_or_else(|| io::Error::other("replayed command returned no command result"))?;
    let exact = first.committed_events() == replayed.committed_events();
    Ok(command_observation(
        if replayed.disposition() == CommandDisposition::Replayed {
            DaemonCommandOutcome::Replayed
        } else {
            DaemonCommandOutcome::Rejected
        },
        replayed.committed_events().map_or(0, peritus_app_protocol::CommittedEventRange::count),
        exact,
        exact,
        0,
        0,
        0,
    ))
}

pub(super) fn conflicting_command() -> io::Result<DaemonConformanceObservation> {
    let (_environment, mut process, mut client) = established(45)?;
    let session = client.context().session_id();
    let first = command::genesis(session, 46, b"conflict-key", 0x22)?;
    let second = command::genesis(session, 47, b"conflict-key", 0x22)?;
    let first_result = command_result(&mut client, first.binding())
        .map_err(|error| {
            io::Error::other(format!("first command: {error}: {}", process.diagnostic()))
        })?
        .ok_or_else(|| io::Error::other("initial conflicting-command setup returned no result"))?;
    if first_result.disposition() != CommandDisposition::Committed {
        return Err(io::Error::other("initial conflicting-command setup did not commit"));
    }
    let conflict = command_result(&mut client, second.binding())
        .map_err(|error| {
            io::Error::other(format!("second command: {error}: {}", process.diagnostic()))
        })?
        .ok_or_else(|| io::Error::other("conflicting command returned no command result"))?;
    let exact = conflict.disposition() == CommandDisposition::Rejected
        && conflict.error().is_some_and(|error| error.code() == AppErrorCode::IdempotencyConflict);
    Ok(command_observation(
        if exact { DaemonCommandOutcome::Conflict } else { DaemonCommandOutcome::Rejected },
        0,
        false,
        false,
        0,
        0,
        0,
    ))
}

pub(super) fn stale_revision() -> io::Result<DaemonConformanceObservation> {
    let (_environment, _process, mut client) = established(48)?;
    let session = client.context().session_id();
    let genesis = command::genesis(session, 49, b"stale-genesis", 0x22)?;
    let _ = command_result(&mut client, genesis.binding())?;
    let stale = command::stale_successor(session, &genesis, 50, b"stale-successor")?;
    let result = command_result(&mut client, stale.binding())?
        .ok_or_else(|| io::Error::other("stale command returned no command result"))?;
    Ok(command_observation(
        if result.disposition() == CommandDisposition::Rejected {
            DaemonCommandOutcome::Rejected
        } else {
            DaemonCommandOutcome::Committed
        },
        0,
        false,
        false,
        0,
        0,
        0,
    ))
}

pub(super) fn fresh_hello(seed: u8) -> ClientHello {
    ClientHello::new(
        ProtocolId::new([seed; 16]).expect("nonzero protocol identity"),
        vec![VersionRange::new(1, 0, 0).expect("supported protocol version")],
        Vec::new(),
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "peritus-g0-black-box".to_owned(),
    )
    .expect("fixed compatible hello")
}

pub(super) fn resume_hello(seed: u8, session: SessionId) -> ClientHello {
    ClientHello::new_with_session(
        ProtocolId::new([seed; 16]).expect("nonzero protocol identity"),
        Some(session),
        vec![VersionRange::new(1, 0, 0).expect("supported protocol version")],
        Vec::new(),
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "peritus-g0-black-box-resume".to_owned(),
    )
    .expect("fixed resume hello")
}

fn incompatible_hello(seed: u8) -> ClientHello {
    ClientHello::new(
        ProtocolId::new([seed; 16]).expect("nonzero protocol identity"),
        vec![VersionRange::new(2, 0, 0).expect("unsupported protocol version")],
        Vec::new(),
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "peritus-g0-incompatible".to_owned(),
    )
    .expect("fixed incompatible hello")
}

fn established(
    seed: u8,
) -> io::Result<(TestEnvironment, super::process::DaemonProcess, WireClient)> {
    let environment = TestEnvironment::new()?;
    let process = environment.start()?;
    let client = WireClient::establish(process.endpoint(), fresh_hello(seed))?;
    Ok((environment, process, client))
}

pub(super) fn command_result(
    client: &mut WireClient,
    binding: &CommandBinding,
) -> io::Result<Option<peritus_app_protocol::CommandResult>> {
    let message = client.request_bound(
        AppRequestPayload::SubmitCommand(binding.clone()),
        binding.request_id(),
        binding.correlation_id(),
    )?;
    let AppMessage::Response(response) = message else {
        return Err(io::Error::other("command returned a non-response message"));
    };
    let AppResponsePayload::CommandResult(result) = response.payload() else {
        return Ok(None);
    };
    Ok(Some(result.clone()))
}

fn send_command_may_close(client: &mut WireClient, binding: CommandBinding) -> io::Result<bool> {
    let request = AppRequestEnvelope::new(
        client.context(),
        binding.request_id(),
        binding.correlation_id(),
        AppRequestPayload::SubmitCommand(binding),
    )
    .map_err(super::debug_error)?;
    client.write(&AppMessage::Request(request))?;
    match client.read() {
        Ok(AppMessage::Response(response)) => Ok(matches!(
            response.payload(),
            AppResponsePayload::Error(error) if matches!(error.code(), AppErrorCode::ReadOnly | AppErrorCode::SessionMismatch)
        )),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::UnexpectedEof
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::BrokenPipe
            ) =>
        {
            Ok(true)
        }
        Ok(_) => Ok(false),
        Err(error) => Err(error),
    }
}

const fn command_observation(
    outcome: DaemonCommandOutcome,
    committed_events: u64,
    response_range_exact: bool,
    original_identity_reconciled: bool,
    replacement_commands: u64,
    new_durable_appends: u64,
    new_external_effects: u64,
) -> DaemonConformanceObservation {
    DaemonConformanceObservation::Command(DaemonCommandObservation::new(
        outcome,
        committed_events,
        response_range_exact,
        original_identity_reconciled,
        replacement_commands,
        new_durable_appends,
        new_external_effects,
    ))
}
