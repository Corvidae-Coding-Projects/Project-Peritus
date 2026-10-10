//! Real durable containment restart, dependency repair, and immutable audit identity replays.

mod support;
use peritus_evidence::{
    EvidenceCancellation, EvidenceErrorKind, EvidenceStore, EvidenceStoreOptions,
};
use rusqlite::{Connection, params};
use std::{fs, num::NonZeroUsize};
use support::{Fixture, revision};
const fn one() -> NonZeroUsize {
    NonZeroUsize::MIN
}

#[test]
fn restart_resumes_exact_scan_and_digest_checked_reconciliation_retains_originals() {
    let mut fixture = Fixture::new();
    let rev = revision();
    let original_bytes = b"immutable recovery object";
    let artifact = fixture.finalize(original_bytes);
    let position = fixture.append(&rev, Some(artifact));
    let export = fixture.export();
    let mut store = fixture.evidence_store();
    let first = store
        .admit(
            Fixture::draft(90, rev, position, vec![artifact], vec![]),
            &export,
            &fixture.artifacts,
        )
        .expect("first record");
    let second = store
        .admit(
            Fixture::draft(91, rev, position, vec![artifact], vec![]),
            &export,
            &fixture.artifacts,
        )
        .expect("second record");
    drop(store);
    let connection = Connection::open(&fixture.path).expect("fault injection connection");
    connection
        .execute(
            "DELETE FROM artifact_references WHERE owner_kind=2 AND owner_identity=?1",
            [first.record_digest().as_bytes().as_slice()],
        )
        .expect("remove normalized root");
    let mut pending = EvidenceStore::open_pending(&fixture.path, EvidenceStoreOptions::default())
        .expect("pending startup");
    let cancellation = EvidenceCancellation::new();
    let progress = pending.containment_step(one(), &cancellation).expect("one identity contained");
    assert_eq!((progress.scanned(), progress.contained(), progress.is_complete()), (1, 1, false));
    let quarantine = pending.quarantined(first.id()).expect("audit").expect("isolated identity");
    let cancelled = EvidenceCancellation::new();
    cancelled.cancel();
    assert_eq!(
        pending.containment_step(one(), &cancelled).expect_err("cancel next page").kind(),
        EvidenceErrorKind::Cancelled
    );
    drop(pending);
    let mut resumed = EvidenceStore::open_pending(&fixture.path, EvidenceStoreOptions::default())
        .expect("resume startup");
    assert_eq!(resumed.containment_progress().expect("durable cursor"), progress);
    let next = resumed.containment_step(one(), &cancellation).expect("second identity");
    assert_eq!(next.scan_id(), progress.scan_id());
    assert_eq!(next.scanned(), 2);
    assert!(
        resumed
            .containment_step(one(), &cancellation)
            .expect("complete bounded scan")
            .is_complete()
    );
    assert_eq!(resumed.load(second.id()).expect("healthy identity"), Some(second));
    repair_dependency(&fixture, &connection, &mut resumed, &first, quarantine);
    drop(resumed);
    repeat_corruption(&fixture, &connection, &first, quarantine);
}

fn repair_dependency(
    fixture: &Fixture,
    connection: &Connection,
    resumed: &mut EvidenceStore,
    first: &peritus_evidence::EvidenceRecord,
    quarantine: peritus_evidence::EvidenceQuarantine,
) {
    let cancellation = EvidenceCancellation::new();
    let artifact = first.artifacts()[0];
    let original_bytes = b"immutable recovery object";
    let original_audit = resumed
        .quarantine_audit(quarantine.quarantine_id())
        .expect("audit by permanent identity")
        .expect("audit retained");
    assert!(
        resumed
            .reconcile_quarantined(
                first.id(),
                quarantine.quarantine_id(),
                &fixture.artifacts,
                &cancellation
            )
            .is_err()
    );
    connection.execute("INSERT INTO artifact_references(owner_kind, owner_identity, artifact_digest) VALUES(2, ?1, ?2)", params![first.record_digest().as_bytes().as_slice(), artifact.as_bytes().as_slice()]).expect("repair exact root");
    fs::write(fixture.object_path(artifact), b"Immutable recovery object")
        .expect("same size corrupt artifact");
    assert_eq!(
        resumed
            .reconcile_quarantined(
                first.id(),
                quarantine.quarantine_id(),
                &fixture.artifacts,
                &cancellation
            )
            .expect_err("must hash repaired dependencies")
            .kind(),
        EvidenceErrorKind::CorruptArtifact
    );
    fs::write(fixture.object_path(artifact), original_bytes).expect("restore exact artifact");
    assert_eq!(
        resumed
            .reconcile_quarantined(
                first.id(),
                quarantine.quarantine_id(),
                &fixture.artifacts,
                &cancellation
            )
            .expect("digest checked reconcile"),
        *first
    );
    assert_eq!(
        resumed
            .reconcile_quarantined(
                first.id(),
                quarantine.quarantine_id(),
                &fixture.artifacts,
                &cancellation
            )
            .expect("idempotent exact reconcile"),
        *first
    );
    let repaired_audit = resumed
        .quarantine_audit(quarantine.quarantine_id())
        .expect("audit after repair")
        .expect("retained");
    assert_eq!(repaired_audit.record_bytes_sha256(), original_audit.record_bytes_sha256());
    assert_eq!(repaired_audit.quarantine_id(), original_audit.quarantine_id());
    assert_eq!(repaired_audit.reconciled_record_digest(), Some(first.record_digest()));
}

fn repeat_corruption(
    fixture: &Fixture,
    connection: &Connection,
    first: &peritus_evidence::EvidenceRecord,
    quarantine: peritus_evidence::EvidenceQuarantine,
) {
    let cancellation = EvidenceCancellation::new();
    let mut reopened = fixture.evidence_store();
    assert_eq!(reopened.load(first.id()).expect("repaired restart"), Some(first.clone()));
    assert_eq!(reopened.quarantine_count().expect("retained history"), 1);
    connection
        .execute(
            "UPDATE peritus_evidence_records SET record_bytes=X'01' WHERE evidence_id=?1",
            [first.id().as_bytes().as_slice()],
        )
        .expect("second corruption");
    drop(reopened);
    reopened = fixture.evidence_store();
    let later = reopened.quarantined(first.id()).expect("second audit").expect("new quarantine");
    assert_ne!(later.quarantine_id(), quarantine.quarantine_id());
    assert!(
        reopened
            .reconcile_quarantined(
                first.id(),
                quarantine.quarantine_id(),
                &fixture.artifacts,
                &cancellation
            )
            .is_err()
    );
    assert!(
        reopened
            .rebuild_quarantined(
                first.id(),
                later.quarantine_id(),
                b"wrong bytes",
                &fixture.artifacts,
                &cancellation
            )
            .is_err()
    );
    assert_eq!(
        reopened
            .rebuild_quarantined(
                first.id(),
                later.quarantine_id(),
                &first.canonical_bytes(),
                &fixture.artifacts,
                &cancellation
            )
            .expect("exact original canonical reconstruction"),
        *first
    );
    assert_eq!(reopened.quarantine_count().expect("both observations retained"), 2);
    assert!(
        reopened
            .quarantine_audit(quarantine.quarantine_id())
            .expect("old observation retained")
            .is_some()
    );
    connection
        .execute(
            "UPDATE peritus_evidence_quarantine SET record_bytes=X'FFFF' WHERE quarantine_id=?1",
            [later.quarantine_id().digest().as_bytes().as_slice()],
        )
        .expect("tamper audit bytes");
    assert_eq!(
        reopened
            .quarantine_audit(later.quarantine_id())
            .expect_err("tamper-evident original audit")
            .kind(),
        EvidenceErrorKind::CorruptCatalog
    );
}

#[test]
fn malformed_raw_identity_is_contained_and_auditable_without_a_fake_evidence_id() {
    let mut fixture = Fixture::new();
    let rev = revision();
    let position = fixture.append(&rev, None);
    let export = fixture.export();
    let mut store = fixture.evidence_store();
    store
        .admit(Fixture::draft(92, rev, position, vec![], vec![]), &export, &fixture.artifacts)
        .expect("record");
    drop(store);
    let connection = Connection::open(&fixture.path).expect("fault connection");
    connection
        .execute("UPDATE peritus_evidence_records SET evidence_id=?1", [[0_u8; 16].as_slice()])
        .expect("reserved raw identity");
    let reopened = fixture.evidence_store();
    let identities = reopened.quarantine_identities(None, 10).expect("audit identities");
    assert_eq!(identities.len(), 1);
    let audit = reopened
        .quarantine_audit(identities[0])
        .expect("malformed-key audit")
        .expect("retained bytes");
    assert_eq!(audit.evidence_id(), None);
    assert_eq!(audit.reconciled_record_digest(), None);
    assert_eq!(audit.raw_evidence_id_sha256(), peritus_codec::sha256(&[0_u8; 16]));
}

#[test]
fn legacy_quarantine_migration_preserves_the_exact_audit_identity_and_bytes() {
    let mut fixture = Fixture::new();
    let rev = revision();
    let position = fixture.append(&rev, None);
    let export = fixture.export();
    let mut store = fixture.evidence_store();
    let record = store
        .admit(Fixture::draft(93, rev, position, vec![], vec![]), &export, &fixture.artifacts)
        .expect("record");
    drop(store);
    let connection = Connection::open(&fixture.path).expect("legacy fixture connection");
    connection
        .execute("UPDATE peritus_evidence_records SET record_bytes=X'42'", [])
        .expect("corrupt record");
    let store = fixture.evidence_store();
    let audit = store.quarantined(record.id()).expect("audit").expect("contained");
    drop(store);
    connection.execute_batch("ALTER TABLE peritus_evidence_quarantine RENAME TO saved_quarantine;
        CREATE TABLE peritus_evidence_quarantine (
            evidence_id BLOB PRIMARY KEY NOT NULL CHECK(length(evidence_id)=16),
            quarantine_digest BLOB NOT NULL UNIQUE CHECK(length(quarantine_digest)=32),
            record_digest BLOB NOT NULL, global_position INTEGER NOT NULL, event_id BLOB NOT NULL,
            batch_hash BLOB NOT NULL, revision_digest BLOB NOT NULL, record_bytes BLOB NOT NULL,
            detected_error TEXT NOT NULL CHECK(length(detected_error)>0)
        ) STRICT, WITHOUT ROWID;
        INSERT INTO peritus_evidence_quarantine SELECT evidence_id, quarantine_digest, record_digest,
            global_position, event_id, batch_hash, revision_digest, record_bytes, detected_error FROM saved_quarantine;
        DROP TABLE saved_quarantine;").expect("install exact v1 quarantine schema");
    let mut reopened = fixture.evidence_store();
    assert_eq!(reopened.quarantined(record.id()).expect("migrated audit"), Some(audit));
    assert_eq!(
        reopened
            .rebuild_quarantined(
                record.id(),
                audit.quarantine_id(),
                &record.canonical_bytes(),
                &fixture.artifacts,
                &EvidenceCancellation::new()
            )
            .expect("repair migrated observation"),
        record
    );
    assert!(
        reopened
            .quarantine_audit(audit.quarantine_id())
            .expect("original audit retained")
            .is_some()
    );
}

#[test]
fn rebuilding_with_a_changed_journal_dependency_rolls_back_every_reconstructed_row() {
    let mut fixture = Fixture::new();
    let rev = revision();
    let position = fixture.append(&rev, None);
    let export = fixture.export();
    let mut store = fixture.evidence_store();
    let record = store
        .admit(Fixture::draft(94, rev, position, vec![], vec![]), &export, &fixture.artifacts)
        .expect("record");
    drop(store);
    let connection = Connection::open(&fixture.path).expect("failure injection");
    connection
        .execute("UPDATE peritus_evidence_records SET record_bytes=X'43'", [])
        .expect("damage active bytes");
    let mut store = fixture.evidence_store();
    let audit = store.quarantined(record.id()).expect("audit").expect("contained");
    let frame: Vec<u8> = connection
        .query_row(
            "SELECT frame FROM events WHERE global_position=?1",
            [i64::try_from(position).expect("position")],
            |row| row.get(0),
        )
        .expect("original journal frame");
    connection
        .execute(
            "UPDATE events SET frame=X'44' WHERE global_position=?1",
            [i64::try_from(position).expect("position")],
        )
        .expect("damage journal dependency");
    assert!(
        store
            .rebuild_quarantined(
                record.id(),
                audit.quarantine_id(),
                &record.canonical_bytes(),
                &fixture.artifacts,
                &EvidenceCancellation::new()
            )
            .is_err()
    );
    let raw: Vec<u8> = connection
        .query_row("SELECT record_bytes FROM peritus_evidence_records", [], |row| row.get(0))
        .expect("read rollback");
    assert_eq!(raw, [0x43]);
    assert_eq!(
        store
            .quarantined(record.id())
            .expect("resolution rolled back")
            .expect("audit")
            .reconciled_record_digest(),
        None
    );
    connection
        .execute(
            "UPDATE events SET frame=?1 WHERE global_position=?2",
            params![frame, i64::try_from(position).expect("position")],
        )
        .expect("repair exact journal bytes");
    assert_eq!(
        store
            .rebuild_quarantined(
                record.id(),
                audit.quarantine_id(),
                &record.canonical_bytes(),
                &fixture.artifacts,
                &EvidenceCancellation::new()
            )
            .expect("repair after exact dependency restored"),
        record
    );
}

#[test]
fn pending_scan_preserves_cursor_under_external_writer_contention_and_resumes() {
    let mut fixture = Fixture::new();
    let rev = revision();
    let position = fixture.append(&rev, None);
    let export = fixture.export();
    let mut store = fixture.evidence_store();
    let record = store
        .admit(Fixture::draft(95, rev, position, vec![], vec![]), &export, &fixture.artifacts)
        .expect("record");
    drop(store);
    let mut store = EvidenceStore::open_pending(
        &fixture.path,
        EvidenceStoreOptions::new(std::time::Duration::ZERO),
    )
    .expect("pending scan");
    let initial = store.containment_progress().expect("initial cursor");
    let writer = Connection::open(&fixture.path).expect("independent writer");
    writer.execute_batch("BEGIN IMMEDIATE").expect("writer runs between scan steps");
    let error = store
        .containment_step(one(), &EvidenceCancellation::new())
        .expect_err("cursor commit contention remains retryable");
    assert_eq!(error.recovery(), peritus_evidence::RecoveryAction::Retry);
    assert_eq!(store.containment_progress().expect("no lost progress"), initial);
    // Normal readers are still usable while this unrelated writer holds the WAL write lock.
    assert_eq!(store.load(record.id()).expect("concurrent checked reader"), Some(record));
    writer.execute_batch("ROLLBACK").expect("release writer");
    let advanced =
        store.containment_step(one(), &EvidenceCancellation::new()).expect("continue same scan");
    assert_eq!(advanced.scan_id(), initial.scan_id());
    assert_eq!(advanced.scanned(), 1);
}
