use std::time::Duration;

use crate::{
    AggregateId, AggregateKey, AggregateKind, AppendPlan, AppendRequest, ArtifactDependency,
    CommandResolution, CredentialRegistryInstall, EventDraft, ExactFrame, HeadExpectation,
    JournalErrorKind, OutboxDraft, OutboxId, StateInstall, StoreId,
};
use peritus_approval::CredentialRegistrySnapshot;
use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_codec::{CodecLimits, decode_frame, encode_frame};
use peritus_types::{CommandId, EventId, EventSequence, RevisionNumber, Sha256Digest};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use super::{SqliteJournal, SqliteJournalOptions};

mod outbox_tests;
mod recovery_tests;

fn id16(value: u8) -> [u8; 16] {
    [value; 16]
}

fn store_id() -> StoreId {
    StoreId::new(id16(1)).expect("store identity")
}

pub(super) fn command(value: u8) -> CommandId {
    CommandId::new(id16(value)).expect("command identity")
}

pub(super) fn event(value: u8) -> EventId {
    EventId::new(id16(value)).expect("event identity")
}

pub(super) fn key(kind: AggregateKind, value: u8) -> AggregateKey {
    AggregateKey::new(kind, AggregateId::new(id16(value)).expect("aggregate identity"))
}

pub(super) fn frame(value: u8) -> ExactFrame {
    ExactFrame::new(
        encode_frame(300, 1, &[value, value.wrapping_add(1)], CodecLimits::PRODUCTION)
            .expect("canonical frame"),
    )
    .expect("checked exact frame")
}

pub(super) fn draft(
    aggregate: AggregateKey,
    sequence: u64,
    event_id: EventId,
    previous: Option<EventId>,
    value: u8,
) -> EventDraft {
    EventDraft::new(
        aggregate,
        EventSequence::new(sequence).expect("positive sequence"),
        event_id,
        previous,
        frame(value),
        Sha256Digest::new([value.wrapping_add(40); 32]),
        Vec::new(),
    )
    .expect("event draft")
}

pub(super) fn plan(
    command_id: CommandId,
    request_digest: Sha256Digest,
    head: HeadExpectation,
    events: Vec<EventDraft>,
) -> AppendPlan {
    AppendRequest::new(
        store_id(),
        command_id,
        request_digest,
        vec![head],
        events,
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("valid append plan")
}

pub(super) fn open(temp: &TempDir) -> SqliteJournal {
    SqliteJournal::open(
        temp.path().join("journal.sqlite3"),
        store_id(),
        SqliteJournalOptions { busy_timeout: Duration::from_millis(250) },
    )
    .expect("open journal")
}

fn write_shared_artifact(temp: &TempDir) -> Sha256Digest {
    let artifact_bytes = vec![90; 123];
    let artifact = Sha256Digest::new(Sha256::digest(&artifact_bytes).into());
    let artifact_config = StoreConfig::new(temp.path().join("artifacts"), 1_024, 4_096)
        .expect("artifact config")
        .with_database_path(temp.path().join("journal.sqlite3"))
        .expect("shared catalog path");
    let artifact_store = ArtifactStore::open(artifact_config).expect("shared artifact store");
    artifact_store
        .begin_write(WriteRequest::new(
            ArtifactDigest::from_sha256(artifact),
            123,
            123,
            MediaType::new("application/octet-stream").expect("media type"),
            EncryptionMetadata::unencrypted(),
            event(19),
        ))
        .and_then(|mut writer| {
            writer.write_chunk(&artifact_bytes)?;
            writer.finalize()
        })
        .expect("finalized artifact metadata");
    artifact
}

#[test]
fn hardened_connection_and_atomic_full_batch_are_observable() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let settings = journal.settings().expect("SQLite settings");
    assert_eq!(settings.journal_mode.to_ascii_lowercase(), "wal");
    assert_eq!(settings.synchronous, 2);
    assert!(settings.foreign_keys);
    assert_eq!(settings.busy_timeout_ms, 250);
    assert!(settings.defensive);

    let aggregate = key(AggregateKind::Kernel, 10);
    let first_event = event(20);
    let exact_first = frame(1);
    let exact_second = frame(2);
    let artifact = write_shared_artifact(&temp);
    let events = vec![
        EventDraft::new(
            aggregate,
            EventSequence::first(),
            first_event,
            None,
            exact_first.clone(),
            Sha256Digest::new([41; 32]),
            Vec::new(),
        )
        .expect("first event"),
        EventDraft::new(
            aggregate,
            EventSequence::new(2).expect("second sequence"),
            event(21),
            Some(first_event),
            exact_second.clone(),
            Sha256Digest::new([42; 32]),
            vec![first_event],
        )
        .expect("second event"),
    ];
    let state = StateInstall::new(1, b"kernel/current".to_vec(), None, 1, vec![7, 8, 9])
        .expect("state install");
    let registry_snapshot = CredentialRegistrySnapshot::new(RevisionNumber::first(), Vec::new())
        .expect("checked registry snapshot");
    let registry_digest = registry_snapshot.digest().expect("registry digest");
    let registry_payload = registry_snapshot.canonical_bytes().expect("registry payload");
    let registry =
        CredentialRegistryInstall::new(None, 1, &registry_snapshot).expect("registry install");
    let outbox_id = OutboxId::new(id16(70)).expect("outbox identity");
    let outbox = OutboxDraft::new(outbox_id, "provider.audit".to_owned(), vec![4, 5, 6], 3)
        .expect("outbox draft");
    let append = AppendRequest::new(
        store_id(),
        command(30),
        Sha256Digest::new([30; 32]),
        vec![HeadExpectation::Absent(aggregate)],
        events,
        vec![state],
        vec![ArtifactDependency::new(artifact)],
        None,
        Some(registry),
        vec![outbox],
    )
    .plan()
    .expect("complete plan");
    let committed = journal.append(append).expect("atomic commit");
    assert_eq!(committed.first_position(), 1);
    assert_eq!(committed.last_position(), 2);
    assert_eq!(committed.records()[0].frame_bytes(), exact_first.bytes());
    assert_eq!(committed.records()[1].frame_bytes(), exact_second.bytes());
    assert_eq!(journal.head(aggregate).expect("head").expect("present").sequence().get(), 2);

    let current = journal.current_credential_registry().expect("current registry");
    assert_eq!(current.revision(), 1);
    assert_eq!(current.digest(), registry_digest);
    assert_eq!(
        decode_frame(current.snapshot_bytes(), CodecLimits::PRODUCTION)
            .expect("registry frame")
            .payload(),
        registry_payload
    );
    let claimed = journal.claim_outbox(10, 20).expect("claim succeeds").expect("message exists");
    assert_eq!(claimed.id(), outbox_id);
    assert_eq!(claimed.payload(), &[4, 5, 6]);
    journal
        .acknowledge_outbox(outbox_id, claimed.fence().expect("claim fence"))
        .expect("acknowledge");
    journal
        .acknowledge_outbox(outbox_id, claimed.fence().expect("claim fence"))
        .expect("idempotent acknowledgement");

    let report = journal.integrity_scan().expect("integrity scan");
    assert_eq!(report.event_count(), 2);
    assert_eq!(report.aggregate_count(), 1);
    let export = journal.integrity_export().expect("integrity export");
    assert_eq!(export.artifact_references().len(), 1);
    assert_eq!(export.artifact_references()[0].batch_hash(), committed.batch_hash());
    assert_eq!(export.artifact_references()[0].first_position(), 1);
    assert_eq!(export.artifact_references()[0].last_position(), 2);
    assert_eq!(export.artifact_references()[0].artifact_digest(), artifact);
}

#[test]
fn stale_head_rejects_every_row_in_the_later_plan() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let aggregate = key(AggregateKind::Budget, 11);
    journal
        .append(plan(
            command(31),
            Sha256Digest::new([31; 32]),
            HeadExpectation::Absent(aggregate),
            vec![draft(aggregate, 1, event(31), None, 1)],
        ))
        .expect("genesis commit");
    let old_head = journal.head(aggregate).expect("head read").expect("head exists");
    journal
        .append(plan(
            command(32),
            Sha256Digest::new([32; 32]),
            HeadExpectation::Present(old_head),
            vec![draft(aggregate, 2, event(32), Some(event(31)), 2)],
        ))
        .expect("head advance");

    let outbox_id = OutboxId::new(id16(99)).expect("outbox identity");
    let stale = AppendRequest::new(
        store_id(),
        command(33),
        Sha256Digest::new([33; 32]),
        vec![HeadExpectation::Present(old_head)],
        vec![draft(aggregate, 2, event(33), Some(event(31)), 3)],
        vec![StateInstall::new(2, b"stale".to_vec(), None, 1, vec![1]).expect("state install")],
        Vec::new(),
        None,
        None,
        vec![OutboxDraft::new(outbox_id, "stale.target".into(), vec![1], 1).expect("outbox")],
    )
    .plan()
    .expect("stale plan remains structurally valid");
    assert_eq!(
        journal.append(stale).expect_err("must reject stale head").kind(),
        JournalErrorKind::StaleHead
    );
    assert!(matches!(
        journal
            .resolve_command(command(33), Sha256Digest::new([33; 32]))
            .expect("resolve stale command"),
        CommandResolution::DefinitelyAbsent
    ));
    assert!(journal.claim_outbox(1, 2).expect("no stale outbox").is_none());
    assert_eq!(journal.integrity_scan().expect("integrity").event_count(), 2);
}

#[test]
fn failed_second_insert_rolls_back_first_event_and_head() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let existing = key(AggregateKind::Kernel, 12);
    let duplicate_id = event(40);
    journal
        .append(plan(
            command(40),
            Sha256Digest::new([40; 32]),
            HeadExpectation::Absent(existing),
            vec![draft(existing, 1, duplicate_id, None, 1)],
        ))
        .expect("existing event");

    let new_aggregate = key(AggregateKind::Lease, 13);
    let failed_command = command(41);
    let failed = plan(
        failed_command,
        Sha256Digest::new([41; 32]),
        HeadExpectation::Absent(new_aggregate),
        vec![
            draft(new_aggregate, 1, event(41), None, 2),
            draft(new_aggregate, 2, duplicate_id, Some(event(41)), 3),
        ],
    );
    assert_eq!(
        journal.append(failed).expect_err("unique failure").kind(),
        JournalErrorKind::Storage
    );
    assert!(journal.head(new_aggregate).expect("head lookup").is_none());
    assert!(matches!(
        journal
            .resolve_command(failed_command, Sha256Digest::new([41; 32]))
            .expect("resolve failed command"),
        CommandResolution::DefinitelyAbsent
    ));
    assert_eq!(journal.integrity_scan().expect("remaining integrity").event_count(), 1);
}

#[test]
fn missing_artifact_fails_before_authoritative_rows_exist() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let aggregate = key(AggregateKind::Kernel, 19);
    let command_id = command(44);
    let digest = Sha256Digest::new([44; 32]);
    let append = AppendRequest::new(
        store_id(),
        command_id,
        digest,
        vec![HeadExpectation::Absent(aggregate)],
        vec![draft(aggregate, 1, event(44), None, 4)],
        vec![
            StateInstall::new(3, b"must-not-install".to_vec(), None, 1, vec![4])
                .expect("state install"),
        ],
        vec![ArtifactDependency::new(Sha256Digest::new([200; 32]))],
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("structurally valid plan");
    assert_eq!(
        journal.append(append).expect_err("artifact is absent").kind(),
        JournalErrorKind::MissingArtifact
    );
    assert!(journal.head(aggregate).expect("head lookup").is_none());
    assert!(matches!(
        journal.resolve_command(command_id, digest).expect("resolve command"),
        CommandResolution::DefinitelyAbsent
    ));
}

#[test]
fn lost_acknowledgement_resolves_and_exact_retry_does_not_append() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let aggregate = key(AggregateKind::Approval, 14);
    let command_id = command(50);
    let digest = Sha256Digest::new([50; 32]);
    let make_plan = || {
        plan(
            command_id,
            digest,
            HeadExpectation::Absent(aggregate),
            vec![draft(aggregate, 1, event(50), None, 4)],
        )
    };
    assert_eq!(
        journal
            .append_losing_acknowledgement(make_plan())
            .expect_err("simulated lost acknowledgement")
            .kind(),
        JournalErrorKind::IndeterminateCommit
    );
    let resolved = journal.resolve_command(command_id, digest).expect("resolve command");
    assert!(matches!(resolved, CommandResolution::Committed(_)));
    let replay = journal.append(make_plan()).expect("exact replay");
    assert_eq!(replay.first_position(), 1);
    assert_eq!(replay.last_position(), 1);

    let conflict = plan(
        command_id,
        Sha256Digest::new([51; 32]),
        HeadExpectation::Absent(key(AggregateKind::Kernel, 15)),
        vec![draft(key(AggregateKind::Kernel, 15), 1, event(51), None, 5)],
    );
    assert_eq!(
        journal.append(conflict).expect_err("digest conflict").kind(),
        JournalErrorKind::IdempotencyConflict
    );
    assert_eq!(journal.integrity_scan().expect("integrity").event_count(), 1);
}
