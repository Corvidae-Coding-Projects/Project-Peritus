//! Durable manifest recovery and publication regressions.

use super::*;

#[test]
fn claim_without_manifest_is_an_explicit_indeterminate_recovery_entry() {
    let registry = TestRegistry::new();
    let identity = identity();
    let process_id = identity.process_id();
    let claims = registry.registry().join("claims-v1");
    std::fs::create_dir(&claims).expect("claim directory");
    persist_claim(&claims, &identity, digest(30), digest(31)).expect("persist orphan claim");

    let store = ProcessStore::open(registry.registry(), registry.workspace()).expect("open store");
    let report = store.reconcile(&mut NoProbe).expect("reconcile orphan claim");
    assert_eq!(report.entries().len(), 1);
    assert_eq!(report.entries()[0].process_id(), process_id);
    assert_eq!(report.entries()[0].disposition(), RecoveryDisposition::Indeterminate);
    assert!(!report.entries()[0].signal_sent());
    assert!(!report.all_terminal());
}

#[test]
fn quarantined_process_records_remain_visible_after_reopen() {
    let registry = TestRegistry::new();
    let store = ProcessStore::open(registry.registry(), registry.workspace()).expect("open store");
    let corrupt = store.root().join("claims-v1").join("corrupt.claim");
    std::fs::write(corrupt, b"not a canonical process claim").expect("write corrupt record");
    drop(store);

    let first = ProcessStore::open(registry.registry(), registry.workspace()).expect("quarantine");
    assert_eq!(first.quarantined_records().len(), 1);
    drop(first);

    let reopened = ProcessStore::open(registry.registry(), registry.workspace()).expect("reopen");
    assert_eq!(reopened.quarantined_records().len(), 1);
    let report = reopened.reconcile(&mut NoProbe).expect("reconcile quarantine");
    assert_eq!(report.quarantined_records(), 1);
    assert!(report.entries().is_empty());
    assert!(!report.all_terminal());
}

#[test]
fn interrupted_manifest_replacement_restores_previous_and_removes_staging() {
    let registry = TestRegistry::new();
    let identity = identity();
    let process_id = identity.process_id();
    prepare_closed_manifest(&registry, &identity, digest(31));
    let manifests = registry.registry().join("manifests-v1");
    let stem = crate::registry_storage::hex(process_id.as_bytes());
    let current = manifests.join(format!("{stem}.manifest"));
    let previous = manifests.join(format!("{stem}.previous"));
    let staging = manifests.join(format!("{stem}.staging"));
    let expected = std::fs::read(&current).expect("current manifest bytes");
    std::fs::rename(&current, &previous).expect("simulate preserved prior manifest");
    std::fs::write(&staging, b"interrupted replacement").expect("simulate staging file");

    let store = ProcessStore::open(registry.registry(), registry.workspace())
        .expect("recover interrupted manifest replacement");

    assert_eq!(std::fs::read(&current).expect("restored manifest"), expected);
    assert!(!previous.exists());
    assert!(!staging.exists());
    let report = store.reconcile(&mut NoProbe).expect("reconcile restored manifest");
    assert_eq!(report.entries().len(), 1);
    assert_eq!(report.entries()[0].process_id(), process_id);
}

#[test]
fn completed_manifest_replacement_discards_stale_previous() {
    let registry = TestRegistry::new();
    let identity = identity();
    let process_id = identity.process_id();
    prepare_closed_manifest(&registry, &identity, digest(31));
    let manifests = registry.registry().join("manifests-v1");
    let stem = crate::registry_storage::hex(process_id.as_bytes());
    let current = manifests.join(format!("{stem}.manifest"));
    let previous = manifests.join(format!("{stem}.previous"));
    std::fs::copy(&current, &previous).expect("simulate stale prior manifest");

    ProcessStore::open(registry.registry(), registry.workspace())
        .expect("clean completed manifest replacement");

    assert!(current.exists());
    assert!(!previous.exists());
}

#[test]
fn manifest_storage_faults_are_reached_truthful_and_recoverable() {
    use std::io::ErrorKind;

    let cases = [
        (StorageFaultPoint::StagingWrite, ErrorKind::PermissionDenied),
        (StorageFaultPoint::StagingSync, ErrorKind::StorageFull),
        (StorageFaultPoint::PreserveRename, ErrorKind::PermissionDenied),
        (StorageFaultPoint::PublishRename, ErrorKind::PermissionDenied),
        (StorageFaultPoint::DirectorySync, ErrorKind::StorageFull),
        (StorageFaultPoint::BackupDelete, ErrorKind::PermissionDenied),
    ];
    for (point, kind) in cases {
        for reproduction in 1..=3 {
            let registry = TestRegistry::new();
            let identity = identity();
            let process_id = identity.process_id();
            prepare_closed_manifest(&registry, &identity, digest(31));
            let manifests = registry.registry().join("manifests-v1");
            let current = manifests
                .join(format!("{}.manifest", crate::registry_storage::hex(process_id.as_bytes())));
            let manifest = ExecutionManifest::decode(
                &std::fs::read(&current).expect("current manifest bytes"),
            )
            .expect("decode owner manifest");
            fault_control::schedule(point, 1, kind);

            let error = write_manifest(&manifests, &manifest)
                .expect_err("scheduled storage fault must remain visible");

            fault_control::verify_hit();
            assert_eq!(error.code(), ErrorCode::Persistence);
            let store = ProcessStore::open(registry.registry(), registry.workspace())
                .expect("recover faulted replacement");
            let report = store.reconcile(&mut NoProbe).expect("reconcile recovered manifest");
            assert_eq!(report.entries().len(), 1, "{point:?} reproduction {reproduction}");
            assert_eq!(report.entries()[0].process_id(), process_id);
        }
    }
}

#[test]
fn unmatched_storage_fault_occurrence_is_a_negative_control() {
    let registry = TestRegistry::new();
    let identity = identity();
    let process_id = identity.process_id();
    prepare_closed_manifest(&registry, &identity, digest(31));
    let manifests = registry.registry().join("manifests-v1");
    let current =
        manifests.join(format!("{}.manifest", crate::registry_storage::hex(process_id.as_bytes())));
    let manifest = ExecutionManifest::decode(&std::fs::read(current).expect("manifest bytes"))
        .expect("decode manifest");
    fault_control::schedule(StorageFaultPoint::StagingWrite, 2, std::io::ErrorKind::Other);

    write_manifest(&manifests, &manifest).expect("unmatched occurrence must not inject");

    fault_control::verify_missed();
}

#[test]
fn claim_manifest_digest_mismatch_blocks_probe_and_terminal_classification() {
    let registry = TestRegistry::new();
    let identity = identity();
    let process_id = identity.process_id();
    prepare_closed_manifest(&registry, &identity, digest(31));
    let claim = registry
        .registry()
        .join("claims-v1")
        .join(format!("{}.claim", crate::registry_storage::hex(process_id.as_bytes())));
    std::fs::remove_file(claim).expect("remove correlated claim");
    persist_claim(&registry.registry().join("claims-v1"), &identity, digest(30), digest(99))
        .expect("persist mismatched claim");

    let store = ProcessStore::open(registry.registry(), registry.workspace()).expect("open store");
    let report = store.reconcile(&mut NoProbe).expect("reconcile mismatch");
    assert_eq!(report.entries().len(), 1);
    assert_eq!(report.entries()[0].process_id(), process_id);
    assert_eq!(report.entries()[0].disposition(), RecoveryDisposition::Indeterminate);
    assert!(!report.all_terminal());
}

#[test]
fn complete_terminal_and_artifact_publication_survive_reopen() {
    let registry = TestRegistry::new();
    let identity = identity();
    let process_id = identity.process_id();
    let plan_digest = digest(31);
    prepare_closed_manifest(&registry, &identity, plan_digest);
    let store = ProcessStore::open(registry.registry(), registry.workspace()).expect("open store");
    let terminal = terminal(process_id, plan_digest);
    store.record_terminal(process_id, &terminal).expect("persist terminal");
    let reopened = ProcessStore::open(registry.registry(), registry.workspace()).expect("reopen");
    assert_eq!(reopened.terminal_result(process_id).expect("terminal result"), terminal);

    let stream = terminal.output().streams()[0];
    let published = artifact(stream, 50);
    let complete =
        store.record_artifact_publication(process_id, published, true).expect("publish artifact");
    assert!(complete.artifact_publication_complete());
    assert_eq!(complete.artifacts(), &[published]);
    assert_eq!(
        store.record_artifact_publication(process_id, published, true).expect("repeat artifact"),
        complete
    );

    let conflicting = artifact(stream, 51);
    let error = store
        .record_artifact_publication(process_id, conflicting, true)
        .expect_err("conflicting artifact must fail");
    assert_eq!(error.code(), ErrorCode::Artifact);
    let reopened = ProcessStore::open(registry.registry(), registry.workspace()).expect("reopen");
    assert_eq!(reopened.terminal_result(process_id).expect("published terminal"), complete);
}

#[test]
fn zero_retained_publication_completion_is_durable_and_idempotent() {
    let registry = TestRegistry::new();
    let identity = identity();
    let process_id = identity.process_id();
    let plan_digest = digest(61);
    prepare_empty_closed_manifest(&registry, &identity, plan_digest);
    let store = ProcessStore::open(registry.registry(), registry.workspace()).expect("open store");
    let mut terminal = TerminalResult::new(
        process_id,
        plan_digest,
        TerminalDisposition::Exited,
        OsExitObservation::Code(0),
        None,
        EscalationRecord::new(false, false, true),
        Some(ProcessInstant::from_millis(1)),
        ProcessInstant::from_millis(2),
        OutputSummary::new(Vec::new(), 0),
        Vec::new(),
        true,
        true,
        TerminalRecovery::OriginalOwner,
    );
    terminal.mark_artifact_failure();
    store.record_terminal(process_id, &terminal).expect("persist incomplete terminal");

    let completed =
        store.complete_artifact_publication(process_id).expect("complete empty publication");
    assert!(completed.artifact_publication_complete());
    assert!(completed.artifacts().is_empty());
    assert_eq!(
        store.complete_artifact_publication(process_id).expect("repeat empty completion"),
        completed
    );
    let reopened = ProcessStore::open(registry.registry(), registry.workspace()).expect("reopen");
    assert_eq!(reopened.terminal_result(process_id).expect("completed terminal"), completed);
}
