//! Large immutable snapshots and publication recovery through the native control store.

use super::*;
use crate::product_control::storage::checkpoints::snapshot::{
    SnapshotFaultPoint, inject_snapshot_fault,
};
use std::io::Write as _;

fn captured(bytes: &[u8]) -> tempfile::TempPath {
    let mut file = tempfile::NamedTempFile::new().expect("capture");
    file.write_all(bytes).expect("bytes");
    file.into_temp_path()
}
fn value(index: u32, revision: u64, paths: Vec<CheckpointPath>) -> UserCheckpoint {
    UserCheckpoint::new(
        CheckpointId::new(
            *operation(index, revision, ControlIntent::PinConversation { pinned: true })
                .id()
                .as_bytes(),
        )
        .expect("ID"),
        "saved files".to_owned(),
        CheckpointReferences::new(revision, 0, 0, None),
        paths,
        Vec::new(),
        Vec::new(),
    )
    .expect("checkpoint")
}
fn version(bytes: &[u8]) -> CheckpointFileVersion {
    CheckpointFileVersion::present(sha256(bytes), bytes.len() as u64, CheckpointFileMode::Regular)
}

#[test]
fn manual_snapshot_over_1024_paths_and_8_mib_restores_after_reopen() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("conversation");
    let large = vec![8_u8; peritus_patch::MAX_FILE_BYTES + 1];
    let mut paths = vec![CheckpointPath::new("large".to_owned(), version(&large)).expect("path")];
    let mut bodies = vec![Some(captured(&large))];
    for index in 0..peritus_patch::MAX_PATCH_OPERATIONS {
        paths.push(
            CheckpointPath::new(format!("small-{index:04}"), version(b"saved")).expect("path"),
        );
        bodies.push(Some(captured(b"saved")));
    }
    let checkpoint = value(2, 1, paths);
    let change = operation(2, 1, ControlIntent::CreateCheckpoint(checkpoint.clone()));
    let receipt = journal.accept_checkpoint_snapshots(&change, &bodies).expect("capture");
    drop(bodies);
    drop(journal);
    let mut journal = store(root.path());
    assert_eq!(
        journal
            .accept_checkpoint_snapshots(&change, &[])
            .expect("original receipt without recapture"),
        receipt
    );
    let recovered = journal
        .load_checkpoint(create().conversation(), checkpoint.id())
        .expect("replay")
        .expect("checkpoint");
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let operations = recovered
        .paths()
        .iter()
        .enumerate()
        .map(|(index, path)| {
            peritus_patch::PatchOperation::create_snapshot(
                peritus_patch::WorkspacePath::new(path.path()).expect("path"),
                journal
                    .checkpoint_snapshot(checkpoint.id(), index, path.checkpoint())
                    .expect("body")
                    .expect("present"),
            )
        })
        .collect();
    let patch = peritus_patch::PatchSet::from_snapshot(
        WorkspaceId::new([4; 16]).expect("workspace"),
        peritus_types::Generation::first(),
        peritus_types::RevisionNumber::first(),
        operations,
    )
    .expect("restore transaction");
    let plan = patch
        .plan(
            WorkspaceId::new([4; 16]).expect("workspace"),
            peritus_types::Generation::first(),
            peritus_types::RevisionNumber::first(),
        )
        .expect("binding");
    peritus_patch::apply_patch(workspace.path(), transactions.path(), &plan).expect("restore all");
    assert_eq!(std::fs::read(workspace.path().join("large")).expect("large"), large);
    for index in 0..peritus_patch::MAX_PATCH_OPERATIONS {
        assert_eq!(
            std::fs::read(workspace.path().join(format!("small-{index:04}"))).expect("small"),
            b"saved"
        );
    }
}

#[test]
fn publication_crashes_preserve_committed_roots_and_release_unaccepted_storage() {
    for point in [
        SnapshotFaultPoint::BeforeChunkFinalization,
        SnapshotFaultPoint::BeforeRootPublication,
        SnapshotFaultPoint::AfterRootPublication,
    ] {
        let root = tempfile::tempdir().expect("root");
        let mut journal = store(root.path());
        journal.accept(&create()).expect("conversation");
        let first = value(
            2,
            1,
            vec![CheckpointPath::new("first".to_owned(), version(b"retained")).expect("path")],
        );
        journal
            .accept_checkpoint_snapshots(
                &operation(2, 1, ControlIntent::CreateCheckpoint(first.clone())),
                &[Some(captured(b"retained"))],
            )
            .expect("baseline");
        let usage = journal.checkpoint_artifacts.quota_snapshot(0).expect("usage");
        let second = value(
            3,
            2,
            vec![CheckpointPath::new("second".to_owned(), version(b"interrupted")).expect("path")],
        );
        let change = operation(3, 2, ControlIntent::CreateCheckpoint(second.clone()));
        inject_snapshot_fault(point);
        journal
            .accept_checkpoint_snapshots(&change, &[Some(captured(b"interrupted"))])
            .expect_err("crash boundary");
        drop(journal);
        let journal = store(root.path());
        assert!(
            journal
                .load_checkpoint(create().conversation(), first.id())
                .expect("baseline intact")
                .is_some()
        );
        let accepted = point == SnapshotFaultPoint::AfterRootPublication;
        assert_eq!(
            journal
                .load_checkpoint(create().conversation(), second.id())
                .expect("second")
                .is_some(),
            accepted
        );
        assert_eq!(journal.resolve(&change).expect("receipt").is_some(), accepted);
        if !accepted {
            assert_eq!(
                journal.checkpoint_artifacts.quota_snapshot(0).expect("reclaimed storage"),
                usage
            );
        }
        assert_eq!(
            std::fs::read_dir(root.path().join("checkpoint-artifacts/publishing"))
                .expect("markers")
                .count(),
            0
        );
    }
}

#[test]
fn restore_evidence_larger_than_a_journal_value_is_durable_and_crash_recoverable() {
    use peritus_product_runner::control::{RestoreId, RestoreOperation, RestoreStatus};

    for point in [
        SnapshotFaultPoint::BeforeChunkFinalization,
        SnapshotFaultPoint::BeforeRootPublication,
        SnapshotFaultPoint::AfterRootPublication,
    ] {
        let root = tempfile::tempdir().expect("root");
        let mut journal = store(root.path());
        journal.accept(&create()).expect("conversation");
        let checkpoint = value(2, 1, Vec::new());
        journal
            .accept_checkpoint_snapshots(
                &operation(2, 1, ControlIntent::CreateCheckpoint(checkpoint.clone())),
                &[],
            )
            .expect("checkpoint");
        let recovery = value(4, 2, Vec::new());
        let restore = RestoreId::new(
            *operation(3, 2, ControlIntent::PinConversation { pinned: false }).id().as_bytes(),
        )
        .expect("restore ID");
        let prepared = RestoreOperation::prepared(
            restore,
            checkpoint.id(),
            sha256(b"preview"),
            sha256(b"plan"),
            recovery.id(),
        )
        .expect("restore");
        journal
            .accept_restore_snapshots(
                &operation(3, 2, ControlIntent::PrepareRestore { restore: prepared, recovery }),
                &[],
            )
            .expect("prepare");
        let baseline = journal.checkpoint_artifacts.quota_snapshot(0).expect("baseline usage");
        let evidence = vec![19_u8; 16 * 1024 * 1024 + 1];
        let digest = sha256(&evidence);
        let settle = operation(
            5,
            3,
            ControlIntent::SettleRestore {
                restore,
                status: RestoreStatus::RecoveryRequired,
                conflicts: Vec::new(),
                transaction_manifest_digest: Some(digest.into_bytes()),
                seal_recovery: false,
            },
        );
        inject_snapshot_fault(point);
        journal
            .accept_restore_settlement(&settle, Some(evidence.clone()))
            .expect_err("publication interrupted");
        drop(journal);
        let mut journal = store(root.path());
        let accepted = point == SnapshotFaultPoint::AfterRootPublication;
        assert_eq!(journal.resolve(&settle).expect("receipt").is_some(), accepted);
        if !accepted {
            assert_eq!(
                journal.checkpoint_artifacts.quota_snapshot(0).expect("reclaimed usage"),
                baseline
            );
        }
        let receipt = journal
            .accept_restore_settlement(&settle, Some(evidence))
            .expect("resume publication or resolve original receipt");
        assert_eq!(
            journal
                .accept_restore_settlement(&settle, None)
                .expect("idempotent receipt without resupplying evidence"),
            receipt
        );
        drop(journal);
        let journal = store(root.path());
        let record =
            journal.load(create().conversation()).expect("verified replay").expect("record");
        assert_eq!(record.restores()[0].status(), RestoreStatus::RecoveryRequired);
        assert_eq!(record.restores()[0].transaction_manifest_digest(), Some(digest));
        assert_eq!(record.revision(), 4);
    }
}
