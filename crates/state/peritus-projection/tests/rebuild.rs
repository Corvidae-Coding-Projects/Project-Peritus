//! File-backed projection replay and generation-swap integration tests.

#[path = "rebuild/journal_fixture.rs"]
mod journal_fixture;

use peritus_codec::{CodecLimits, encode_frame, encode_message};
use peritus_journal::{
    AggregateKind, AppendRequest, ArtifactDependency, EventDraft, ExactFrame, HeadExpectation,
};
use peritus_kernel::KernelEventKind;
use peritus_projection::{
    ArtifactReferenceProjection, EvidenceCatalogProjection, JournalCatalogProjection,
    LifecycleProjection, Projection, ProjectionErrorKind, ProjectionStore, RepairAction,
    RepairReason, StoreOptions, rebuild_from_genesis, replay_artifact_references,
    replay_from_genesis,
};
use peritus_protocol::{KernelEventDto, KernelSubjectDto};
use peritus_types::{EventSequence, SessionId, Sha256Digest};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};

use journal_fixture::{
    Fixture, command_id, event_id, key, phase_frame, relabel_schema, revision,
    scheduler_event_frame, store_id,
};

#[test]
fn shadow_rebuild_restart_checksum_and_atomic_swap() {
    let mut fixture = Fixture::new();
    let aggregate = key(AggregateKind::Kernel, 10);
    fixture.append(aggregate, phase_frame(), 90);
    let export = fixture.export();
    let projection = JournalCatalogProjection::new().expect("projection");
    let first = rebuild_from_genesis(&projection, &export).expect("first rebuild");
    let repeated = rebuild_from_genesis(&projection, &export).expect("repeat rebuild");
    assert_eq!(first.payload(), repeated.payload());
    assert_eq!(first.invariant_digest(), repeated.invariant_digest());

    let mut store = ProjectionStore::open(&fixture.path, StoreOptions::default()).expect("store");
    let generation = store.install_shadow(&first, None).expect("install generation");
    assert_eq!(generation.get(), 1);
    assert_eq!(
        store.plan_startup(projection.schema(), export.report()).expect("startup plan"),
        RepairAction::Reuse(generation)
    );
    let same =
        store.install_shadow(&repeated, Some(generation)).expect("identical rebuild is reused");
    assert_eq!(same, generation);
    assert_eq!(store.generation_count(projection.schema()).expect("count"), 1);

    drop(store);
    let restarted = ProjectionStore::open(&fixture.path, StoreOptions::default()).expect("restart");
    let active =
        restarted.load_active(projection.schema()).expect("load active").expect("active exists");
    assert_eq!(active.generation(), generation);
    assert_eq!(active.payload(), first.payload());
    assert!(active.payload_is_valid());
}

#[test]
fn stale_checkpoint_rebuilds_and_stale_swap_is_atomic() {
    let mut fixture = Fixture::new();
    let aggregate = key(AggregateKind::Kernel, 11);
    fixture.append(aggregate, phase_frame(), 91);
    let projection = JournalCatalogProjection::new().expect("projection");
    let first_export = fixture.export();
    let first = rebuild_from_genesis(&projection, &first_export).expect("rebuild");
    let mut store = ProjectionStore::open(&fixture.path, StoreOptions::default()).expect("store");
    let generation = store.install_shadow(&first, None).expect("install");

    fixture.append(aggregate, phase_frame(), 91);
    let second_export = fixture.export();
    assert_eq!(
        store.plan_startup(projection.schema(), second_export.report()).expect("stale plan"),
        RepairAction::RebuildFromGenesis(RepairReason::PositionChanged)
    );
    let second = rebuild_from_genesis(&projection, &second_export).expect("second rebuild");
    let conflict = store.install_shadow(&second, None).expect_err("stale CAS");
    assert_eq!(conflict.kind(), ProjectionErrorKind::Conflict);
    assert_eq!(store.generation_count(projection.schema()).expect("count"), 1);
    assert_eq!(
        store.load_active(projection.schema()).expect("active").expect("present").generation(),
        generation
    );
    let next = store.install_shadow(&second, Some(generation)).expect("swap");
    assert_eq!(next.get(), 2);
    assert_eq!(store.generation_count(projection.schema()).expect("count"), 2);
}

#[test]
fn corrupt_payload_is_replaced_by_a_new_active_generation() {
    let mut fixture = Fixture::new();
    fixture.append(key(AggregateKind::Kernel, 12), phase_frame(), 92);
    let projection = JournalCatalogProjection::new().expect("projection");
    let export = fixture.export();
    let candidate = rebuild_from_genesis(&projection, &export).expect("rebuild");
    let mut store = ProjectionStore::open(&fixture.path, StoreOptions::default()).expect("store");
    store.install_shadow(&candidate, None).expect("install");
    drop(store);

    let connection = Connection::open(&fixture.path).expect("direct connection");
    connection
        .execute(
            "UPDATE peritus_projection_generations SET payload = ?1 WHERE projection_name = ?2",
            params![b"corrupt".as_slice(), projection.schema().identity().name().as_str()],
        )
        .expect("corrupt payload");
    drop(connection);
    let mut store = ProjectionStore::open(&fixture.path, StoreOptions::default()).expect("reopen");
    assert_eq!(
        store.plan_startup(projection.schema(), export.report()).expect("repair plan"),
        RepairAction::RebuildFromGenesis(RepairReason::PayloadCorrupt)
    );
    let corrupt = store.load_active(projection.schema()).expect("load corrupt").expect("active");
    assert!(!corrupt.payload_is_valid());
    let rebuilt = rebuild_from_genesis(&projection, &export).expect("repeat rebuild");
    let replacement = store
        .install_shadow(&rebuilt, Some(corrupt.generation()))
        .expect("replace corrupt generation");
    assert_eq!(replacement.get(), corrupt.generation().get() + 1);
    let active = store.load_active(projection.schema()).expect("load repaired").expect("active");
    assert_eq!(active.generation(), replacement);
    assert!(active.payload_is_valid());
    assert_eq!(active.payload(), rebuilt.payload());
    assert_eq!(store.generation_count(projection.schema()).expect("count"), 2);
}

#[test]
fn replay_rejects_unknown_typed_invalid_and_stale_revision_records() {
    let mut unknown = Fixture::new();
    unknown.append(
        key(AggregateKind::Kernel, 13),
        encode_frame(300, 1, &[1, 2], CodecLimits::PRODUCTION).expect("unknown frame"),
        93,
    );
    let projection = JournalCatalogProjection::new().expect("projection");
    let error = replay_from_genesis(&projection, &unknown.export()).expect_err("unknown family");
    assert_eq!(error.kind(), ProjectionErrorKind::UnsupportedFamily);

    let mut invalid = Fixture::new();
    invalid.append(
        key(AggregateKind::Kernel, 14),
        encode_frame(3, 1, &[1], CodecLimits::PRODUCTION).expect("framed invalid payload"),
        94,
    );
    let lifecycle = LifecycleProjection::new().expect("lifecycle");
    let error = replay_from_genesis(&lifecycle, &invalid.export()).expect_err("typed invalid");
    assert_eq!(error.kind(), ProjectionErrorKind::InvalidFrame);

    let mut stale = Fixture::new();
    let aggregate = key(AggregateKind::Kernel, 15);
    stale.append(aggregate, phase_frame(), 95);
    stale.append(aggregate, phase_frame(), 96);
    let error = replay_from_genesis(&projection, &stale.export()).expect_err("revision change");
    assert_eq!(error.kind(), ProjectionErrorKind::StaleRevision);
}

#[test]
fn replay_accepts_supported_scheduler_schemas_only() {
    let projection = JournalCatalogProjection::new().expect("projection");
    for version in [1, 2] {
        let mut fixture = Fixture::new();
        fixture.append(
            key(AggregateKind::Scheduler, u8::try_from(version).expect("fixture version")),
            scheduler_event_frame(version),
            93,
        );
        replay_from_genesis(&projection, &fixture.export())
            .expect("supported scheduler event schema replays");
    }

    let mut unsupported = Fixture::new();
    unsupported.append(key(AggregateKind::Scheduler, 30), scheduler_event_frame(3), 94);
    let error = replay_from_genesis(&projection, &unsupported.export())
        .expect_err("unsupported scheduler event schema rejects");
    assert_eq!(error.kind(), ProjectionErrorKind::UnsupportedSchema);

    let mut non_scheduler = Fixture::new();
    non_scheduler.append(key(AggregateKind::Kernel, 31), relabel_schema(phase_frame(), 2), 95);
    let error = replay_from_genesis(&projection, &non_scheduler.export())
        .expect_err("non-scheduler family remains schema one only");
    assert_eq!(error.kind(), ProjectionErrorKind::UnsupportedSchema);
}

#[test]
fn lifecycle_fold_checks_envelope_and_evidence_fold_has_no_effect() {
    let mut fixture = Fixture::new();
    let aggregate = key(AggregateKind::Kernel, 16);
    let event = event_id(70);
    let command = command_id(71);
    let dto = KernelEventDto {
        id: event,
        command_id: command,
        sequence: EventSequence::first(),
        previous_event_id: None,
        revision: revision(),
        kind: KernelEventKind::SessionOpened,
        subject: KernelSubjectDto::Session(SessionId::new([16; 16]).expect("session id")),
    };
    let draft = EventDraft::new(
        aggregate,
        EventSequence::first(),
        event,
        None,
        ExactFrame::new(encode_message(&dto, CodecLimits::PRODUCTION).expect("event frame"))
            .expect("exact"),
        Sha256Digest::new([97; 32]),
        Vec::new(),
    )
    .expect("draft");
    let plan = AppendRequest::new(
        store_id(),
        command,
        Sha256Digest::new([98; 32]),
        vec![HeadExpectation::Absent(aggregate)],
        vec![draft],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("plan");
    fixture.journal.append(plan).expect("append");
    let export = fixture.export();
    let lifecycle = replay_from_genesis(&LifecycleProjection::new().expect("projection"), &export)
        .expect("lifecycle replay");
    assert_eq!(lifecycle.state().len(), 1);
    let evidence =
        replay_from_genesis(&EvidenceCatalogProjection::new().expect("evidence"), &export)
            .expect("evidence replay");
    assert!(evidence.state().is_empty());
}

#[test]
fn journal_corruption_is_rejected_before_projection_replay() {
    let mut fixture = Fixture::new();
    fixture.append(key(AggregateKind::Kernel, 17), phase_frame(), 99);
    let connection = Connection::open(&fixture.path).expect("direct connection");
    connection
        .execute("UPDATE events SET frame = ?1 WHERE global_position = 1", [b"broken".as_slice()])
        .expect("corrupt journal frame");
    drop(connection);
    let error = fixture.journal.integrity_export().expect_err("integrity must reject corruption");
    assert_eq!(error.kind(), peritus_journal::JournalErrorKind::CorruptJournal);
}

#[test]
fn artifact_projection_uses_actual_committed_dependency_digest() {
    use peritus_artifact_store::{
        ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
    };

    let mut fixture = Fixture::new();
    let content = vec![42_u8; 97];
    let digest = Sha256Digest::new(Sha256::digest(&content).into());
    let artifact_store = ArtifactStore::open(
        StoreConfig::new(fixture.temp.path().join("artifacts"), 1_024, 4_096)
            .expect("artifact config")
            .with_database_path(&fixture.path)
            .expect("shared database"),
    )
    .expect("artifact store");
    artifact_store
        .begin_write(WriteRequest::new(
            ArtifactDigest::from_sha256(digest),
            97,
            97,
            MediaType::new("application/octet-stream").expect("media type"),
            EncryptionMetadata::unencrypted(),
            event_id(80),
        ))
        .and_then(|mut writer| {
            writer.write_chunk(&content)?;
            writer.finalize()
        })
        .expect("finalize artifact");

    let aggregate = key(AggregateKind::Kernel, 18);
    let frame = ExactFrame::new(phase_frame()).expect("exact frame");
    assert_ne!(frame.digest(), digest);
    let draft = EventDraft::new(
        aggregate,
        EventSequence::first(),
        event_id(81),
        None,
        frame,
        Sha256Digest::new([100; 32]),
        Vec::new(),
    )
    .expect("draft");
    let plan = AppendRequest::new(
        store_id(),
        command_id(81),
        Sha256Digest::new([101; 32]),
        vec![HeadExpectation::Absent(aggregate)],
        vec![draft],
        Vec::new(),
        vec![ArtifactDependency::new(digest)],
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("artifact append plan");
    fixture.journal.append(plan).expect("artifact append");
    let export = fixture.export();
    let replay = replay_artifact_references(
        &ArtifactReferenceProjection::new().expect("projection"),
        &export,
    )
    .expect("artifact replay");
    let entry = replay.state().get(digest).expect("actual digest projected");
    assert_eq!(entry.first_position(), 1);
    assert_eq!(entry.last_position(), 1);
    assert_eq!(entry.owner_count(), 1);
}
