//! Historical frame import and raw C0 append fixtures.

use std::fs;
use std::path::{Path, PathBuf};

use peritus_codec::sha256;
use peritus_evidence::revision_digest;
use peritus_journal::{
    AppendRequest, CommittedBatch, EventDraft, ExactFrame, HeadExpectation, SqliteJournal,
    SqliteJournalOptions, StateInstall, StoreId,
};
use peritus_scheduler::{
    SCHEDULER_STATE_NAMESPACE, SchedulerCommand, SchedulerEvent, SchedulerSemantics,
    SchedulerState, decode_scheduler_command, decode_scheduler_event, encode_scheduler_command,
    encode_scheduler_event, encode_scheduler_state, replay, scheduler_aggregate_key,
    scheduler_state_key,
};
use peritus_types::{CommandId, EventId};
use tempfile::TempDir;

use super::{HISTORY_FILES, LIMITS};

pub struct History {
    pub commands: Vec<Vec<u8>>,
    pub events: Vec<Vec<u8>>,
    pub checkpoint: Vec<u8>,
}

impl History {
    pub fn load(scenario: &str) -> Result<Self, std::io::Error> {
        let root = fixture_root().join(scenario);
        let mut commands = Vec::with_capacity(HISTORY_FILES.len());
        let mut events = Vec::with_capacity(HISTORY_FILES.len());
        for suffix in HISTORY_FILES {
            commands.push(fs::read(root.join(format!("command-{suffix}.bin")))?);
            events.push(fs::read(root.join(format!("event-{suffix}.bin")))?);
        }
        Ok(Self { commands, events, checkpoint: fs::read(root.join("checkpoint.bin"))? })
    }
}

pub struct Imported {
    pub command: SchedulerCommand,
    pub event: SchedulerEvent,
    pub state: SchedulerState,
    pub batch: CommittedBatch,
}

pub fn import_historical(
    journal: &mut SqliteJournal,
    history: &History,
    index: usize,
    events: &mut Vec<SchedulerEvent>,
) -> Result<Imported, Box<dyn std::error::Error>> {
    let command = decode_scheduler_command(&history.commands[index], LIMITS)?;
    let event = decode_scheduler_event(&history.events[index], LIMITS)?;
    assert_eq!(command.semantics(), SchedulerSemantics::LegacyQueueV1);
    assert_eq!(event.semantics(), SchedulerSemantics::LegacyQueueV1);
    assert_eq!(command.command_id(), event.command_id());
    assert_eq!(encode_scheduler_command(&command, LIMITS)?, history.commands[index]);
    assert_eq!(encode_scheduler_event(&event, LIMITS)?, history.events[index]);
    events.push(event.clone());
    let state = replay(events)?;
    assert_eq!(state.state_digest(), event.successor_state_digest());
    let state_bytes = encode_scheduler_state(&state, LIMITS)?;
    let installed_state_bytes = if index + 1 == history.events.len() {
        assert_eq!(state_bytes, history.checkpoint);
        history.checkpoint.clone()
    } else {
        state_bytes
    };

    let aggregate = scheduler_aggregate_key(command.run_id())?;
    let expectation = journal
        .head(aggregate)?
        .map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let state_key = scheduler_state_key(command.run_id());
    let expected_revision = journal
        .state_record(SCHEDULER_STATE_NAMESPACE, &state_key)?
        .map(|record| record.revision());
    let draft = EventDraft::new(
        aggregate,
        event.sequence(),
        event.id(),
        event.previous_event(),
        ExactFrame::new(history.events[index].clone())?,
        revision_digest(&event.revision()),
        Vec::new(),
    )?;
    let install = StateInstall::new(
        SCHEDULER_STATE_NAMESPACE,
        state_key,
        expected_revision,
        event.sequence().get(),
        installed_state_bytes,
    )?;
    let request = AppendRequest::new(
        journal.store_id(),
        command.command_id(),
        sha256(&history.commands[index]),
        vec![expectation],
        vec![draft],
        vec![install],
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let batch = journal.append(request.plan()?)?;
    Ok(Imported { command, event, state, batch })
}

pub fn append_raw(
    journal: &mut SqliteJournal,
    command_bytes: &[u8],
    event_bytes: Vec<u8>,
    state_bytes: Vec<u8>,
) -> Result<CommittedBatch, Box<dyn std::error::Error>> {
    let command = decode_scheduler_command(command_bytes, LIMITS)?;
    let event = decode_scheduler_event(&event_bytes, LIMITS)?;
    assert_eq!(command.command_id(), event.command_id());
    assert_eq!(command.run_id(), event.run_id());

    let aggregate = scheduler_aggregate_key(event.run_id())?;
    let expectation = journal
        .head(aggregate)?
        .map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let state_key = scheduler_state_key(event.run_id());
    let expected_revision = journal
        .state_record(SCHEDULER_STATE_NAMESPACE, &state_key)?
        .map(|record| record.revision());
    let draft = EventDraft::new(
        aggregate,
        event.sequence(),
        event.id(),
        event.previous_event(),
        ExactFrame::new(event_bytes)?,
        revision_digest(&event.revision()),
        Vec::new(),
    )?;
    let install = StateInstall::new(
        SCHEDULER_STATE_NAMESPACE,
        state_key,
        expected_revision,
        event.sequence().get(),
        state_bytes,
    )?;
    let request = AppendRequest::new(
        journal.store_id(),
        command.command_id(),
        sha256(command_bytes),
        vec![expectation],
        vec![draft],
        vec![install],
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    Ok(journal.append(request.plan()?)?)
}

pub fn open(directory: &TempDir) -> Result<SqliteJournal, peritus_journal::JournalError> {
    SqliteJournal::open(journal_path(directory), store_id(), SqliteJournalOptions::default())
}

pub fn journal_path(directory: &TempDir) -> PathBuf {
    directory.path().join("scheduler.sqlite3")
}

pub fn store_id() -> StoreId {
    StoreId::new([90; 16]).expect("fixed nonzero store identity")
}

pub fn command_id(value: u8) -> CommandId {
    CommandId::new([value; 16]).expect("fixed nonzero command identity")
}

pub fn event_id(value: u8) -> EventId {
    EventId::new([value; 16]).expect("fixed nonzero event identity")
}

pub fn relabel_schema(bytes: &[u8], schema_version: u16) -> Vec<u8> {
    let mut relabeled = bytes.to_vec();
    relabeled[8..10].copy_from_slice(&schema_version.to_be_bytes());
    relabeled
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("fixtures/protocol/scheduler-v1-recovery")
}
