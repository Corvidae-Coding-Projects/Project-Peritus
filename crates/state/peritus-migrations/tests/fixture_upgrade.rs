//! Historical shared-database fixture upgrade and journal compatibility tests.

mod support;

use std::fs;

use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, EventDraft, ExactFrame,
    HeadExpectation, SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_migrations::{MigrationEngine, MigrationRegistry, RecoveryState};
use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};
use rusqlite::params;
use sha2::{Digest, Sha256};

use support::{config, create_journal_database, operation, version};

#[test]
fn unversioned_fixture_upgrades_without_damaging_journal_replay_contract() {
    let temp = tempfile::tempdir().expect("tempdir");
    let database = create_journal_database(&temp);
    let connection = rusqlite::Connection::open(&database).expect("fixture connection");
    let current = std::env::current_dir().expect("migration test working directory");
    let fixture = current
        .ancestors()
        .map(|root| root.join("crates/state/peritus-migrations/fixtures/v0.sql"))
        .find(|path| path.is_file())
        .expect("checked-in v0 migration fixture path");
    let schema = fs::read_to_string(fixture).expect("read checked-in v0 migration fixture");
    connection.execute_batch(&schema).expect("install v0 fixture");
    drop(connection);

    let mut engine =
        MigrationEngine::open(config(&temp, database.clone()), MigrationRegistry::current())
            .expect("migration engine");
    let plan = engine.preflight(version(1)).expect("preflight").into_plan();
    let applied = engine.apply(&plan, operation(9)).expect("upgrade fixture");
    assert!(applied.backup_path().expect("required backup").is_file());
    drop(engine);

    let fixture_connection = rusqlite::Connection::open(&database).expect("verify fixture");
    let payload: Vec<u8> = fixture_connection
        .query_row(
            "SELECT fixture_value FROM migration_fixture_payload WHERE fixture_key = 'preserved'",
            [],
            |row| row.get(0),
        )
        .expect("preserved fixture payload");
    assert_eq!(payload, b"peritus");
    drop(fixture_connection);

    let mut journal = SqliteJournal::open(
        &database,
        StoreId::new([1; 16]).expect("store identity"),
        SqliteJournalOptions::default(),
    )
    .expect("journal reopens after migration");
    let report = journal.integrity_scan().expect("journal integrity/replay scan");
    assert_eq!(report.event_count(), 0);
    assert_eq!(report.last_position(), 0);
    drop(journal);

    let mut rollback =
        MigrationEngine::open(config(&temp, database.clone()), MigrationRegistry::current())
            .expect("reopen migration engine for operational rollback");
    let restored = rollback.restore_backup(operation(9)).expect("restore historical fixture");
    assert_eq!(restored.state(), RecoveryState::Restored);
    drop(rollback);

    let restored_connection = rusqlite::Connection::open(&database).expect("restored fixture");
    let version: i64 = restored_connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("restored version");
    let restored_payload: Vec<u8> = restored_connection
        .query_row(
            "SELECT fixture_value FROM migration_fixture_payload WHERE fixture_key = 'preserved'",
            [],
            |row| row.get(0),
        )
        .expect("restored fixture payload");
    assert_eq!(version, 0);
    assert_eq!(restored_payload, b"peritus");
    drop(restored_connection);

    let mut restored_journal = SqliteJournal::open(
        database,
        StoreId::new([1; 16]).expect("store identity"),
        SqliteJournalOptions::default(),
    )
    .expect("journal reopens after rollback restoration");
    assert_eq!(restored_journal.integrity_scan().expect("restored integrity").event_count(), 0);
}

#[test]
fn v1_journal_rows_migrate_byte_exactly_and_new_aggregate_records_append() {
    let temp = tempfile::tempdir().expect("tempdir");
    let database = temp.path().join("journal-v1.sqlite3");
    let connection = rusqlite::Connection::open(&database).expect("v1 connection");
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v1.sql");
    connection
        .execute_batch(&fs::read_to_string(fixture).expect("read v1 fixture"))
        .expect("install frozen v1 schema");
    let preserved = insert_v1_record(&connection);
    drop(connection);

    let mut engine =
        MigrationEngine::open(config(&temp, database.clone()), MigrationRegistry::current())
            .expect("migration engine");
    let plan = engine.preflight(version(4)).expect("v4 preflight").into_plan();
    assert_eq!(plan.current_version(), 1);
    assert!(plan.backup_required());
    let applied = engine.apply(&plan, operation(10)).expect("v4 apply");
    assert!(applied.backup_path().expect("required backup").is_file());
    drop(engine);

    let connection = rusqlite::Connection::open(&database).expect("verify migrated rows");
    let after = read_v1_record(&connection);
    assert_eq!(after, preserved, "migration must not re-encode immutable rows");
    assert_eq!(
        connection
            .query_row("SELECT schema_version FROM store_meta WHERE singleton = 1", [], |row| row
                .get::<_, i64>(
                0
            ))
            .expect("schema version"),
        4
    );
    assert!(
        !connection
            .prepare("PRAGMA foreign_key_check")
            .expect("foreign key statement")
            .exists([])
            .expect("foreign key check")
    );
    drop(connection);

    let mut journal = SqliteJournal::open(
        &database,
        StoreId::new([1; 16]).expect("store identity"),
        SqliteJournalOptions::default(),
    )
    .expect("schema-v4 journal");
    assert_eq!(journal.integrity_scan().expect("pre-D0 integrity").event_count(), 1);

    let aggregate = AggregateKey::new(
        AggregateKind::Agent,
        AggregateId::new([20; 16]).expect("agent aggregate"),
    );
    let frame = ExactFrame::new(frame(301, &[7, 8, 9])).expect("agent frame");
    let draft = EventDraft::new(
        aggregate,
        EventSequence::first(),
        EventId::new([21; 16]).expect("event"),
        None,
        frame,
        Sha256Digest::new([22; 32]),
        Vec::new(),
    )
    .expect("agent draft");
    let append = AppendRequest::new(
        StoreId::new([1; 16]).expect("store identity"),
        CommandId::new([23; 16]).expect("command"),
        Sha256Digest::new([24; 32]),
        vec![HeadExpectation::Absent(aggregate)],
        vec![draft],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("agent append plan");
    journal.append(append).expect("agent append");
    append_new_aggregate(&mut journal, AggregateKind::Gate, 30, 31, 32, 51);
    append_new_aggregate(&mut journal, AggregateKind::Trace, 40, 41, 42, 60);
    append_new_aggregate(&mut journal, AggregateKind::Review, 50, 51, 52, 54);
    assert_eq!(journal.integrity_scan().expect("post-upgrade integrity").event_count(), 5);
}

#[test]
fn v3_fixture_preserves_every_historical_aggregate_tag_through_d2_migration() {
    let temp = tempfile::tempdir().expect("tempdir");
    let database = temp.path().join("journal-v3.sqlite3");
    let connection = rusqlite::Connection::open(&database).expect("v3 connection");
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/v3.sql");
    connection
        .execute_batch(&fs::read_to_string(fixture).expect("read v3 fixture"))
        .expect("install frozen v3 schema");
    for tag in 1_u8..=8 {
        insert_v3_record(&connection, tag, tag.saturating_mul(10), 40 + u16::from(tag));
    }
    let preserved = snapshot_v3_rows(&connection);
    drop(connection);

    let mut engine =
        MigrationEngine::open(config(&temp, database.clone()), MigrationRegistry::current())
            .expect("migration engine");
    let plan = engine.preflight(version(4)).expect("v4 preflight").into_plan();
    assert_eq!(plan.current_version(), 3);
    assert!(plan.backup_required());
    let applied = engine.apply(&plan, operation(11)).expect("v4 apply");
    assert!(applied.backup_path().expect("required backup").is_file());
    drop(engine);

    let connection = rusqlite::Connection::open(&database).expect("migrated connection");
    assert_eq!(snapshot_v3_rows(&connection), preserved);
    assert_eq!(
        connection
            .query_row("SELECT schema_version FROM store_meta WHERE singleton = 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .expect("schema version"),
        4
    );
    drop(connection);

    let mut journal = SqliteJournal::open(
        &database,
        StoreId::new([1; 16]).expect("store identity"),
        SqliteJournalOptions::default(),
    )
    .expect("schema-v4 journal");
    assert_eq!(journal.integrity_scan().expect("migrated integrity").event_count(), 8);
    append_new_aggregate(&mut journal, AggregateKind::Review, 90, 91, 92, 54);
    assert_eq!(journal.integrity_scan().expect("review integrity").event_count(), 9);
    drop(journal);

    let mut rollback =
        MigrationEngine::open(config(&temp, database.clone()), MigrationRegistry::current())
            .expect("rollback engine");
    let restored = rollback.restore_backup(operation(11)).expect("restore v3 backup");
    assert_eq!(restored.state(), RecoveryState::Restored);
    drop(rollback);
    let restored = rusqlite::Connection::open(database).expect("restored v3 fixture");
    assert_eq!(snapshot_v3_rows(&restored), preserved);
    assert_eq!(
        restored
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .expect("restored user version"),
        3
    );
}

fn append_new_aggregate(
    journal: &mut SqliteJournal,
    kind: AggregateKind,
    aggregate_identity: u8,
    event_identity: u8,
    command_identity: u8,
    family: u16,
) {
    let aggregate = AggregateKey::new(
        kind,
        AggregateId::new([aggregate_identity; 16]).expect("aggregate identity"),
    );
    let draft = EventDraft::new(
        aggregate,
        EventSequence::first(),
        EventId::new([event_identity; 16]).expect("event identity"),
        None,
        ExactFrame::new(frame(family, &[aggregate_identity])).expect("domain frame"),
        Sha256Digest::new([event_identity; 32]),
        Vec::new(),
    )
    .expect("domain event draft");
    let plan = AppendRequest::new(
        StoreId::new([1; 16]).expect("store identity"),
        CommandId::new([command_identity; 16]).expect("command identity"),
        Sha256Digest::new([command_identity; 32]),
        vec![HeadExpectation::Absent(aggregate)],
        vec![draft],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("domain append plan");
    journal.append(plan).expect("domain append");
}

fn insert_v3_record(connection: &rusqlite::Connection, kind: u8, identity: u8, family: u16) {
    let position = i64::from(kind);
    let aggregate = [identity; 16];
    let event = [identity.saturating_add(1); 16];
    let command = [identity.saturating_add(2); 16];
    let request = Sha256Digest::new([identity.saturating_add(3); 32]);
    let revision = Sha256Digest::new([identity.saturating_add(4); 32]);
    let frame = frame(family, &[kind, identity]);
    let frame_digest = digest(&frame);
    let event_hash =
        event_hash_for_kind(u16::from(kind), aggregate, event, command, frame_digest, revision);
    let batch_hash = batch_hash(command, request, event_hash);
    connection
        .execute(
            "INSERT INTO events(global_position, event_id, aggregate_kind, aggregate_id, sequence,
             previous_event_id, previous_event_hash, event_hash, command_id, frame_family,
             frame_schema, frame_digest, revision_digest, causal_ids, frame)
             VALUES (?1, ?2, ?3, ?4, 1, NULL, zeroblob(32), ?5, ?6, ?7, 1, ?8, ?9, X'', ?10)",
            params![
                position,
                event,
                i64::from(kind),
                aggregate,
                event_hash.as_bytes(),
                command,
                i64::from(family),
                frame_digest.as_bytes(),
                revision.as_bytes(),
                frame,
            ],
        )
        .expect("insert v3 event");
    connection
        .execute(
            "INSERT INTO aggregate_heads(aggregate_kind, aggregate_id, sequence, event_id, event_hash)
             VALUES (?1, ?2, 1, ?3, ?4)",
            params![i64::from(kind), aggregate, event, event_hash.as_bytes()],
        )
        .expect("insert v3 head");
    connection
        .execute(
            "INSERT INTO commands(command_id, request_digest, first_position, last_position, event_count, batch_hash)
             VALUES (?1, ?2, ?3, ?3, 1, ?4)",
            params![command, request.as_bytes(), position, batch_hash.as_bytes()],
        )
        .expect("insert v3 command");
}

fn snapshot_v3_rows(connection: &rusqlite::Connection) -> (Vec<String>, Vec<String>) {
    let query = |sql: &str| {
        let mut statement = connection.prepare(sql).expect("snapshot statement");
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("snapshot rows")
            .map(|row| row.expect("snapshot row"))
            .collect::<Vec<_>>()
    };
    let heads = query(
        "SELECT printf('%d:%s:%d:%s:%s', aggregate_kind, hex(aggregate_id), sequence,
         hex(event_id), hex(event_hash)) FROM aggregate_heads ORDER BY aggregate_kind, aggregate_id",
    );
    let events = query(
        "SELECT printf('%d:%s:%d:%s:%d:%s:%s:%s:%s:%d:%d:%s:%s:%s:%s', global_position,
         hex(event_id), aggregate_kind, hex(aggregate_id), sequence, ifnull(hex(previous_event_id), ''),
         hex(previous_event_hash), hex(event_hash), hex(command_id), frame_family, frame_schema,
         hex(frame_digest), hex(revision_digest), hex(causal_ids), hex(frame))
         FROM events ORDER BY global_position",
    );
    (heads, events)
}

#[derive(Debug, Eq, PartialEq)]
struct PreservedRows {
    head: (i64, Vec<u8>, i64, Vec<u8>, Vec<u8>),
    event: PreservedEvent,
    state: (i64, Vec<u8>, i64, Vec<u8>, Vec<u8>, i64),
}

#[derive(Debug, Eq, PartialEq)]
struct PreservedEvent {
    global_position: i64,
    event_id: Vec<u8>,
    aggregate_kind: i64,
    aggregate_id: Vec<u8>,
    sequence: i64,
    previous_event_id: Option<Vec<u8>>,
    previous_event_hash: Vec<u8>,
    event_hash: Vec<u8>,
    command_id: Vec<u8>,
    frame_family: i64,
    frame_schema: i64,
    frame_digest: Vec<u8>,
    revision_digest: Vec<u8>,
    causal_ids: Vec<u8>,
    frame: Vec<u8>,
}

fn insert_v1_record(connection: &rusqlite::Connection) -> PreservedRows {
    let aggregate = [2_u8; 16];
    let event = [3_u8; 16];
    let command = [4_u8; 16];
    let request = Sha256Digest::new([5; 32]);
    let revision = Sha256Digest::new([6; 32]);
    let frame = frame(300, &[9, 10]);
    let frame_digest = digest(&frame);
    let event_hash = event_hash(aggregate, event, command, frame_digest, revision);
    let batch_hash = batch_hash(command, request, event_hash);
    connection
        .execute(
            "INSERT INTO events(global_position, event_id, aggregate_kind, aggregate_id, sequence,
         previous_event_id, previous_event_hash, event_hash, command_id, frame_family,
         frame_schema, frame_digest, revision_digest, causal_ids, frame)
         VALUES (1, ?1, 1, ?2, 1, NULL, zeroblob(32), ?3, ?4, 300, 1, ?5, ?6, X'', ?7)",
            params![
                event,
                aggregate,
                event_hash.as_bytes(),
                command,
                frame_digest.as_bytes(),
                revision.as_bytes(),
                frame
            ],
        )
        .expect("insert v1 event");
    connection.execute(
        "INSERT INTO aggregate_heads(aggregate_kind, aggregate_id, sequence, event_id, event_hash)
         VALUES (1, ?1, 1, ?2, ?3)",
        params![aggregate, event, event_hash.as_bytes()],
    ).expect("insert v1 head");
    connection.execute(
        "INSERT INTO commands(command_id, request_digest, first_position, last_position, event_count, batch_hash)
         VALUES (?1, ?2, 1, 1, 1, ?3)",
        params![command, request.as_bytes(), batch_hash.as_bytes()],
    ).expect("insert v1 command");
    let value = b"preserved-state".to_vec();
    let value_digest = digest(&value);
    for table in ["state_records", "state_record_history"] {
        connection.execute(
            &format!("INSERT INTO {table}(namespace, record_key, revision, value_digest, value, producing_position) VALUES (9, X'6b6579', 1, ?1, ?2, 1)"),
            params![value_digest.as_bytes(), value],
        ).expect("insert v1 state");
    }
    read_v1_record(connection)
}

fn read_v1_record(connection: &rusqlite::Connection) -> PreservedRows {
    let head = connection.query_row(
        "SELECT aggregate_kind, aggregate_id, sequence, event_id, event_hash FROM aggregate_heads",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    ).expect("read head");
    let event = connection
        .query_row(
            "SELECT global_position, event_id, aggregate_kind, aggregate_id, sequence,
         previous_event_id, previous_event_hash, event_hash, command_id, frame_family,
         frame_schema, frame_digest, revision_digest, causal_ids, frame FROM events",
            [],
            |row| {
                Ok(PreservedEvent {
                    global_position: row.get(0)?,
                    event_id: row.get(1)?,
                    aggregate_kind: row.get(2)?,
                    aggregate_id: row.get(3)?,
                    sequence: row.get(4)?,
                    previous_event_id: row.get(5)?,
                    previous_event_hash: row.get(6)?,
                    event_hash: row.get(7)?,
                    command_id: row.get(8)?,
                    frame_family: row.get(9)?,
                    frame_schema: row.get(10)?,
                    frame_digest: row.get(11)?,
                    revision_digest: row.get(12)?,
                    causal_ids: row.get(13)?,
                    frame: row.get(14)?,
                })
            },
        )
        .expect("read event");
    let state = connection.query_row(
        "SELECT namespace, record_key, revision, value_digest, value, producing_position FROM state_records",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
    ).expect("read state");
    PreservedRows { head, event, state }
}

fn frame(family: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = b"PRTS".to_vec();
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&family.to_be_bytes());
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&0_u16.to_be_bytes());
    bytes.extend_from_slice(&u32::try_from(payload.len()).expect("small payload").to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::new(Sha256::digest(bytes).into())
}

fn event_hash(
    aggregate: [u8; 16],
    event: [u8; 16],
    command: [u8; 16],
    frame: Sha256Digest,
    revision: Sha256Digest,
) -> Sha256Digest {
    event_hash_for_kind(1, aggregate, event, command, frame, revision)
}

fn event_hash_for_kind(
    kind: u16,
    aggregate: [u8; 16],
    event: [u8; 16],
    command: [u8; 16],
    frame: Sha256Digest,
    revision: Sha256Digest,
) -> Sha256Digest {
    let mut bytes = b"peritus.journal.event.v1\0".to_vec();
    bytes.extend_from_slice(&kind.to_be_bytes());
    bytes.extend_from_slice(&aggregate);
    bytes.extend_from_slice(&1_u64.to_be_bytes());
    bytes.extend_from_slice(&event);
    bytes.push(0);
    bytes.extend_from_slice(&[0; 16]);
    bytes.extend_from_slice(&[0; 32]);
    bytes.extend_from_slice(&command);
    bytes.extend_from_slice(frame.as_bytes());
    bytes.extend_from_slice(revision.as_bytes());
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    digest(&bytes)
}

fn batch_hash(command: [u8; 16], request: Sha256Digest, event: Sha256Digest) -> Sha256Digest {
    let mut bytes = b"peritus.journal.batch.v1\0".to_vec();
    bytes.extend_from_slice(&[1; 16]);
    bytes.extend_from_slice(&command);
    bytes.extend_from_slice(request.as_bytes());
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(event.as_bytes());
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    digest(&bytes)
}
