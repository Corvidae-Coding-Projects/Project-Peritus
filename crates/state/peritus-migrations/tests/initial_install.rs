//! Fresh first-release installation, one-time adoption, and restart coverage.

mod support;

use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, EventDraft, ExactFrame,
    HeadExpectation, SqliteJournal, SqliteJournalOptions, StateInstall, StoreId,
};
use peritus_migrations::{MigrationEngine, MigrationRegistry};
use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

use support::{config, create_journal_database, operation, version};

#[test]
fn fresh_schema_is_complete_and_adopted_once_without_upgrade_or_backup() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = create_journal_database(&temp);
    let connection = rusqlite::Connection::open(&path).expect("inspect initial schema");
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .expect("initial version"),
        1
    );
    assert_eq!(
        connection
            .query_row("SELECT schema_version FROM store_meta", [], |row| row.get::<_, i64>(0))
            .expect("journal version"),
        1
    );
    for name in [
        "aggregate_heads",
        "events",
        "commands",
        "state_records",
        "state_record_history",
        "state_history_nodes",
        "outbox",
        "authority_clock",
        "credential_registry",
        "app_principals",
        "app_sessions",
        "app_commands",
        "app_commands_state",
        "app_prompt_targets",
        "app_prompt_targets_state",
        "app_artifacts",
        "app_workspaces",
    ] {
        assert!(
            connection
                .prepare("SELECT 1 FROM sqlite_schema WHERE name = ?1")
                .expect("schema query")
                .exists([name])
                .expect("schema object"),
            "missing {name}"
        );
    }
    assert!(
        !connection
            .prepare("SELECT 1 FROM pragma_table_info('state_record_history') WHERE name = 'value'")
            .expect("history columns")
            .exists([])
            .expect("no legacy inline column")
    );
    drop(connection);

    let configuration = config(&temp, path.clone());
    let mut engine = MigrationEngine::open(configuration.clone(), MigrationRegistry::current())
        .expect("migration engine");
    assert!(engine.adopt_current_install(operation(8)).expect("adopt initial schema"));
    let plan = engine.preflight(version(1)).expect("initial preflight").into_plan();
    assert_eq!(plan.current_version(), 1);
    assert!(plan.steps().is_empty());
    assert!(!plan.backup_required());
    assert!(!engine.adopt_current_install(operation(8)).expect("repeated adoption is inert"));
    drop(engine);
    let mut reopened = MigrationEngine::open(configuration, MigrationRegistry::current())
        .expect("restart migration engine");
    assert!(!reopened.adopt_current_install(operation(9)).expect("restart adoption is inert"));
    assert!(reopened.reconcile().expect("no pending migrations").actions().is_empty());
    drop(reopened);
    let connection = rusqlite::Connection::open(&path).expect("inspect durable initial marker");
    let marker: (i64, String) = connection
        .query_row("SELECT version, release FROM schema_migrations", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("one marker");
    assert_eq!(marker, (1, "0.0.1".to_owned()));
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get::<_, i64>(0))
            .expect("marker count"),
        1
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM recovery_operations", [], |row| row.get::<_, i64>(0))
            .expect("operation count"),
        0
    );
}

#[test]
fn initial_schema_accepts_all_release_aggregate_families_and_shared_history() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = create_journal_database(&temp);
    let store = StoreId::new([1; 16]).expect("store");
    let mut journal =
        SqliteJournal::open(&path, store, SqliteJournalOptions::default()).expect("fresh journal");
    let kinds = [
        AggregateKind::Kernel,
        AggregateKind::Budget,
        AggregateKind::Lease,
        AggregateKind::Approval,
        AggregateKind::CredentialRegistry,
        AggregateKind::Agent,
        AggregateKind::Gate,
        AggregateKind::Trace,
        AggregateKind::Review,
        AggregateKind::Scheduler,
        AggregateKind::Collaboration,
        AggregateKind::Orchestrator,
        AggregateKind::Harness,
        AggregateKind::Debugger,
        AggregateKind::Evaluation,
        AggregateKind::EvolutionCampaign,
        AggregateKind::ProductionHarness,
        AggregateKind::Application,
    ];
    for (index, kind) in kinds.into_iter().enumerate() {
        let marker = u8::try_from(index + 1).expect("small family registry");
        let aggregate = AggregateKey::new(kind, AggregateId::new([marker; 16]).expect("aggregate"));
        let draft = EventDraft::new(
            aggregate,
            EventSequence::first(),
            EventId::new([marker; 16]).expect("event"),
            None,
            frame(marker),
            Sha256Digest::new([marker; 32]),
            Vec::new(),
        )
        .expect("draft");
        let state = StateInstall::new(u16::from(marker), vec![marker], None, 1, vec![marker; 513])
            .expect("initial state");
        let append = AppendRequest::new(
            store,
            CommandId::new([marker; 16]).expect("command"),
            Sha256Digest::new([marker; 32]),
            vec![HeadExpectation::Absent(aggregate)],
            vec![draft],
            vec![state],
            Vec::new(),
            None,
            None,
            Vec::new(),
        )
        .plan()
        .expect("append plan");
        journal.append(append).expect("append current release family");
    }
    drop(journal);
    let mut restarted = SqliteJournal::open(&path, store, SqliteJournalOptions::default())
        .expect("restart current journal");
    for marker in 1_u8..=18 {
        let state = restarted
            .state_record_revision(u16::from(marker), &[marker], 1)
            .expect("read shared history")
            .expect("state");
        assert_eq!(state.bytes(), &[marker; 513]);
        assert_eq!(state.producing_position(), u64::from(marker));
    }
    assert_eq!(restarted.integrity_scan().expect("complete integrity").event_count(), 18);
}

fn frame(marker: u8) -> ExactFrame {
    let mut bytes = b"PRTS".to_vec();
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&300_u16.to_be_bytes());
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&0_u16.to_be_bytes());
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.push(marker);
    ExactFrame::new(bytes).expect("exact frame")
}
