//! Crash recovery fixture for an admitted command with an indeterminate settlement.

use std::io;

use peritus_app_protocol::{AppErrorCode, CommandDisposition};
use peritus_conformance::{DaemonCommandOutcome, DaemonConformanceObservation};
use peritus_journal::{
    ApplicationCommandAdmission, ApplicationCommandSettlement, ApplicationCommandState,
    ApplicationRequestId, NewApplicationCommand, SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_types::ActorId;

use super::{command_observation, command_result, fresh_hello, resume_hello};
use crate::daemon_conformance_support::{command, process::TestEnvironment, wire::WireClient};

pub fn indeterminate_command() -> io::Result<DaemonConformanceObservation> {
    let environment = TestEnvironment::new()?;
    let mut initial = environment.start()?;
    let client = WireClient::establish(initial.endpoint(), fresh_hello(51))?;
    let session = client.context().session_id();
    let fixture = command::genesis(session, 52, b"indeterminate-command", 0x22)?;
    let binding = fixture.binding().clone();
    drop(client);
    initial.kill_for_restart()?;

    let command_id = binding.frames().envelope().as_domain().command_id();
    let mut journal = SqliteJournal::open(
        environment.database_path(),
        StoreId::new([0x11; 16]).map_err(crate::daemon_conformance_support::debug_error)?,
        SqliteJournalOptions::default(),
    )
    .map_err(crate::daemon_conformance_support::debug_error)?;
    let seeded = NewApplicationCommand::new(
        ActorId::new([0x22; 16]).map_err(crate::daemon_conformance_support::debug_error)?,
        session,
        binding.idempotency_key().as_bytes().to_vec(),
        binding.request_digest().as_sha256(),
        binding.frames().command_frame().digest(),
        ApplicationRequestId::new(binding.request_id().into_bytes())
            .map_err(crate::daemon_conformance_support::debug_error)?,
        command_id,
    )
    .map_err(crate::daemon_conformance_support::debug_error)?;
    let ApplicationCommandAdmission::Inserted(record) =
        journal
            .admit_application_command(seeded)
            .map_err(crate::daemon_conformance_support::debug_error)?
    else {
        return Err(io::Error::other("indeterminate command fixture was not newly admitted"));
    };
    journal
        .settle_application_command(
            command_id,
            record.request_digest(),
            ApplicationCommandSettlement::indeterminate(),
        )
        .map_err(crate::daemon_conformance_support::debug_error)?;
    drop(journal);

    let mut restarted = environment.start()?;
    let mut client = WireClient::establish(restarted.endpoint(), resume_hello(53, session))?;
    let result = command_result(&mut client, &binding)?
        .ok_or_else(|| io::Error::other("indeterminate command returned no command result"))?;
    drop(client);
    restarted.kill_for_restart()?;

    let journal = SqliteJournal::open(
        environment.database_path(),
        StoreId::new([0x11; 16]).map_err(crate::daemon_conformance_support::debug_error)?,
        SqliteJournalOptions::default(),
    )
    .map_err(crate::daemon_conformance_support::debug_error)?;
    let reconciled = journal
        .application_command(command_id)
        .map_err(crate::daemon_conformance_support::debug_error)?
        .ok_or_else(|| io::Error::other("reconciled application command disappeared"))?;
    let connection = rusqlite::Connection::open(environment.database_path())
        .map_err(crate::daemon_conformance_support::debug_error)?;
    let command_count = count_rows(&connection, "SELECT COUNT(*) FROM app_commands")?;
    let event_count = count_rows(&connection, "SELECT COUNT(*) FROM events")?;
    let exact = result.disposition() == CommandDisposition::Rejected
        && result.error().is_some_and(|error| error.code() == AppErrorCode::UnsupportedFamily)
        && reconciled.command_id() == command_id
        && reconciled.request_digest() == binding.request_digest().as_sha256()
        && reconciled.state() == ApplicationCommandState::Rejected;
    Ok(command_observation(
        if exact { DaemonCommandOutcome::Indeterminate } else { DaemonCommandOutcome::Rejected },
        0,
        false,
        exact,
        command_count.saturating_sub(1),
        event_count,
        0,
    ))
}

fn count_rows(connection: &rusqlite::Connection, statement: &str) -> io::Result<u64> {
    u64::try_from(
        connection
            .query_row(statement, [], |row| row.get::<_, i64>(0))
            .map_err(crate::daemon_conformance_support::debug_error)?,
    )
    .map_err(crate::daemon_conformance_support::debug_error)
}
