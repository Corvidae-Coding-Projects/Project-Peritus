//! Shared journal and identity fixtures for projection rebuild tests.

use std::path::{Path, PathBuf};

use peritus_codec::{CodecLimits, encode_frame, encode_message};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, EventDraft, ExactFrame,
    HeadExpectation, SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_kernel::SessionPhase;
use peritus_protocol::LifecyclePhaseDto;
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, EventSequence, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};
use tempfile::TempDir;

pub struct Fixture {
    pub temp: TempDir,
    pub path: PathBuf,
    pub journal: SqliteJournal,
    next: u8,
}

impl Fixture {
    pub fn new() -> Self {
        let temp = TempDir::new().expect("temporary directory");
        let path = temp.path().join("shared.sqlite3");
        let journal = open_journal(&path);
        Self { temp, path, journal, next: 20 }
    }

    pub fn append(&mut self, aggregate: AggregateKey, bytes: Vec<u8>, revision: u8) {
        let head = self.journal.head(aggregate).expect("read head");
        let sequence = head.map_or(1, |value| value.sequence().get() + 1);
        let previous = head.map(peritus_journal::AggregateHead::event_id);
        let event = event_id(self.next);
        let command = command_id(self.next);
        self.next = self.next.checked_add(1).expect("fixture id space");
        let draft = EventDraft::new(
            aggregate,
            EventSequence::new(sequence).expect("sequence"),
            event,
            previous,
            ExactFrame::new(bytes).expect("exact frame"),
            Sha256Digest::new([revision; 32]),
            Vec::new(),
        )
        .expect("draft");
        let expectation = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
        let plan = AppendRequest::new(
            store_id(),
            command,
            Sha256Digest::new([self.next; 32]),
            vec![expectation],
            vec![draft],
            Vec::new(),
            Vec::new(),
            None,
            None,
            Vec::new(),
        )
        .plan()
        .expect("plan");
        self.journal.append(plan).expect("append");
    }

    pub fn export(&mut self) -> peritus_journal::IntegrityExport {
        self.journal.integrity_export().expect("integrity export")
    }
}

fn open_journal(path: &Path) -> SqliteJournal {
    SqliteJournal::open(path, store_id(), SqliteJournalOptions::default()).expect("open journal")
}

pub fn store_id() -> StoreId {
    StoreId::new([1; 16]).expect("store id")
}

pub fn key(kind: AggregateKind, byte: u8) -> AggregateKey {
    AggregateKey::new(kind, AggregateId::new([byte; 16]).expect("aggregate id"))
}

pub fn event_id(byte: u8) -> EventId {
    EventId::new([byte; 16]).expect("event id")
}

pub fn command_id(byte: u8) -> CommandId {
    CommandId::new([byte; 16]).expect("command id")
}

pub fn phase_frame() -> Vec<u8> {
    encode_message(&LifecyclePhaseDto::Session(SessionPhase::Open), CodecLimits::PRODUCTION)
        .expect("phase frame")
}

pub fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new([2; 16]).expect("acceptance id"),
        HarnessId::new([3; 16]).expect("harness id"),
        WorkspaceId::new([4; 16]).expect("workspace id"),
        Generation::new(1).expect("generation"),
        RevisionNumber::new(1).expect("revision"),
        PolicyId::new([5; 16]).expect("policy id"),
        ProviderProfileId::new([6; 16]).expect("provider id"),
    )
}

pub fn scheduler_event_frame(schema_version: u16) -> Vec<u8> {
    let frame = encode_frame(71, 1, &[], CodecLimits::PRODUCTION).expect("scheduler event frame");
    relabel_schema(frame, schema_version)
}

pub fn relabel_schema(mut frame: Vec<u8>, schema_version: u16) -> Vec<u8> {
    frame[8..10].copy_from_slice(&schema_version.to_be_bytes());
    frame
}
