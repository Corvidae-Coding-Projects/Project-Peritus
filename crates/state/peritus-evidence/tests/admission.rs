//! File-backed admission, atomicity, artifact, and restart integration tests.

mod support;

use peritus_evidence::{EvidenceDraft, EvidenceErrorKind, EvidenceKind, EvidenceSource, Freshness};
use peritus_types::Sha256Digest;
use rusqlite::Connection;
use std::fs;
use support::{Fixture, evidence_id, revision};

#[test]
fn valid_admission_is_atomic_idempotent_and_survives_restart() {
    let mut fixture = Fixture::new();
    let revision = revision();
    let artifact = fixture.finalize(b"durable evidence artifact");
    let position = fixture.append(&revision, Some(artifact));
    let export = fixture.export();
    let draft = Fixture::draft(40, revision, position, vec![artifact], Vec::new());
    let mut store = fixture.evidence_store();
    let admitted = store.admit(draft.clone(), &export, &fixture.artifacts).expect("admitted");
    assert_eq!(
        store.admit(draft, &export, &fixture.artifacts).expect("idempotent retry"),
        admitted
    );
    assert_eq!(store.freshness(admitted.id(), &revision).expect("freshness"), Freshness::Current);

    let connection = Connection::open(&fixture.path).expect("inspect database");
    let evidence_rows: i64 = connection
        .query_row("SELECT COUNT(*) FROM peritus_evidence_records", [], |row| row.get(0))
        .expect("evidence count");
    let durable_roots: i64 = connection
        .query_row("SELECT COUNT(*) FROM artifact_references WHERE owner_kind = 2", [], |row| {
            row.get(0)
        })
        .expect("evidence artifact root count");
    assert_eq!((evidence_rows, durable_roots), (1, 1));
    drop(connection);
    drop(store);

    let reopened = support::open_evidence(&fixture.path);
    assert_eq!(reopened.load(admitted.id()).expect("load after restart"), Some(admitted));
}

#[test]
fn exact_retry_after_journal_advance_returns_original_record() {
    let mut fixture = Fixture::new();
    let revision = revision();
    let position = fixture.append(&revision, None);
    let export = fixture.export();
    let draft = Fixture::draft(50, revision, position, Vec::new(), Vec::new());
    let mut store = fixture.evidence_store();
    let admitted = store
        .admit(draft.clone(), &export, &fixture.artifacts)
        .expect("initial evidence admission");

    fixture.append(&revision, None);
    let later_export = fixture.export();
    assert_eq!(
        store
            .admit(draft, &later_export, &fixture.artifacts)
            .expect("exact retry after journal advance"),
        admitted
    );

    let conflicting = EvidenceDraft::new(
        admitted.id(),
        EvidenceKind::new("execution-result").expect("kind"),
        EvidenceSource::new("local-runner").expect("source"),
        revision,
        position,
        Sha256Digest::new([51; 32]),
        Vec::new(),
        Vec::new(),
    )
    .expect("conflicting draft");
    let error = store
        .admit(conflicting, &later_export, &fixture.artifacts)
        .expect_err("conflicting retry remains rejected");
    assert_eq!(error.kind(), EvidenceErrorKind::IdentityConflict);
}

#[test]
fn missing_and_corrupt_artifact_bytes_are_rejected() {
    for (byte, replacement, expected) in [
        (41, None, EvidenceErrorKind::MissingArtifact),
        (42, Some(b"tampered".as_slice()), EvidenceErrorKind::CorruptArtifact),
    ] {
        let mut fixture = Fixture::new();
        let revision = revision();
        let artifact = fixture.finalize(b"immutable artifact bytes");
        let position = fixture.append(&revision, Some(artifact));
        let export = fixture.export();
        let path = fixture.object_path(artifact);
        if let Some(bytes) = replacement {
            fs::write(path, bytes).expect("corrupt artifact fixture");
        } else {
            fs::remove_file(path).expect("remove artifact fixture");
        }
        let draft = Fixture::draft(byte, revision, position, vec![artifact], Vec::new());
        let mut store = fixture.evidence_store();
        let error = store.admit(draft, &export, &fixture.artifacts).expect_err("artifact rejected");
        assert_eq!(error.kind(), expected);
        assert_eq!(store.load(evidence_id(byte)).expect("catalog remains readable"), None);
    }
}

#[test]
fn nonexistent_cause_and_identity_conflict_are_rejected_without_partial_rows() {
    let mut fixture = Fixture::new();
    let revision = revision();
    let position = fixture.append(&revision, None);
    let export = fixture.export();
    let mut store = fixture.evidence_store();
    let invalid = Fixture::draft(43, revision, position, Vec::new(), vec![evidence_id(99)]);
    let error = store.admit(invalid, &export, &fixture.artifacts).expect_err("missing cause");
    assert_eq!(error.kind(), EvidenceErrorKind::InvalidCause);
    assert_eq!(store.load(evidence_id(43)).expect("read"), None);

    let valid = Fixture::draft(44, revision, position, Vec::new(), Vec::new());
    store.admit(valid, &export, &fixture.artifacts).expect("first identity use");
    let conflicting = EvidenceDraft::new(
        evidence_id(44),
        EvidenceKind::new("execution-result").expect("kind"),
        EvidenceSource::new("local-runner").expect("source"),
        revision,
        position,
        Sha256Digest::new([45; 32]),
        Vec::new(),
        Vec::new(),
    )
    .expect("conflicting draft");
    let error =
        store.admit(conflicting, &export, &fixture.artifacts).expect_err("identity conflict");
    assert_eq!(error.kind(), EvidenceErrorKind::IdentityConflict);
}

#[test]
fn canonical_older_ancestry_is_admitted_and_future_causality_is_rejected() {
    let mut fixture = Fixture::new();
    let revision = revision();
    let parent_position = fixture.append(&revision, None);
    let child_position = fixture.append(&revision, None);
    let export = fixture.export();
    let mut store = fixture.evidence_store();

    let parent = store
        .admit(
            Fixture::draft(45, revision, parent_position, Vec::new(), Vec::new()),
            &export,
            &fixture.artifacts,
        )
        .expect("parent evidence");
    let child = store
        .admit(
            Fixture::draft(46, revision, child_position, Vec::new(), vec![parent.id()]),
            &export,
            &fixture.artifacts,
        )
        .expect("older parent accepted");
    assert_eq!(child.causes(), &[parent.id()]);

    let future_parent = store
        .admit(
            Fixture::draft(47, revision, child_position, Vec::new(), Vec::new()),
            &export,
            &fixture.artifacts,
        )
        .expect("future parent fixture");
    let error = store
        .admit(
            Fixture::draft(48, revision, parent_position, Vec::new(), vec![future_parent.id()]),
            &export,
            &fixture.artifacts,
        )
        .expect_err("future cause rejected");
    assert_eq!(error.kind(), EvidenceErrorKind::InvalidCause);
}

#[test]
fn durable_catalog_corruption_is_detected_on_reopen() {
    let mut fixture = Fixture::new();
    let revision = revision();
    let position = fixture.append(&revision, None);
    let export = fixture.export();
    let mut store = fixture.evidence_store();
    let record = store
        .admit(
            Fixture::draft(49, revision, position, Vec::new(), Vec::new()),
            &export,
            &fixture.artifacts,
        )
        .expect("admit evidence");
    drop(store);

    let connection = Connection::open(&fixture.path).expect("open corrupt fixture");
    connection
        .execute(
            "UPDATE peritus_evidence_records SET record_bytes = X'00' WHERE evidence_id = ?1",
            [record.id().as_bytes().as_slice()],
        )
        .expect("corrupt record bytes");
    drop(connection);
    let reopened = support::open_evidence(&fixture.path);
    let error = reopened.load(record.id()).expect_err("catalog corruption rejected");
    assert_eq!(error.kind(), EvidenceErrorKind::CorruptCatalog);
}
