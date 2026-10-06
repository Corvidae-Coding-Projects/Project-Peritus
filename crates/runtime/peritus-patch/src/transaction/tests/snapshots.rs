//! Snapshot restoration remains one transaction beyond inline command bounds.

use super::*;
use crate::SnapshotFile;
use std::io::Write as _;

fn snapshot(bytes: &[u8]) -> SnapshotFile {
    let mut file = tempfile::NamedTempFile::new().expect("snapshot file");
    file.write_all(bytes).expect("snapshot bytes");
    SnapshotFile::from_source(
        std::sync::Arc::new(Saved(file.into_temp_path())),
        peritus_codec::sha256(bytes),
        bytes.len() as u64,
        FileMode::Regular,
    )
}

#[derive(Debug)]
struct Saved(tempfile::TempPath);
impl crate::SnapshotSource for Saved {
    fn open(&self) -> io::Result<Box<dyn io::Read + Send>> {
        Ok(Box::new(std::fs::File::open(&self.0)?))
    }
}

fn snapshot_plan(workspace: &std::path::Path) -> (PatchPlan, Vec<u8>) {
    let before = vec![3_u8; 8 * 1024 * 1024 + 1];
    let after = vec![7_u8; 8 * 1024 * 1024 + 1];
    std::fs::write(workspace.join("large"), &before).expect("original large file");
    let mut operations = vec![
        PatchOperation::replace_snapshot(
            WorkspacePath::new("large").expect("path"),
            Preimage::from_bytes(&before, FileMode::Regular),
            snapshot(&after),
        )
        .expect("snapshot replacement"),
    ];
    for index in 0..1_024 {
        operations.push(PatchOperation::create_snapshot(
            WorkspacePath::new(format!("small-{index:04}")).expect("path"),
            snapshot(b"saved"),
        ));
    }
    let plan = PatchSet::from_snapshot(
        binding().workspace_id(),
        binding().generation(),
        binding().revision(),
        operations,
    )
    .expect("snapshot exceeds inline limits without exceeding physical manifest capacity")
    .plan(binding().workspace_id(), binding().generation(), binding().revision())
    .expect("bound plan");
    (plan, after)
}

#[test]
fn large_snapshot_with_1025_targets_recovers_exactly_after_restart() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let (plan, expected) = snapshot_plan(workspace.path());
    let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
    let result = apply_with_faults(
        workspace.path(),
        transactions.path(),
        &plan,
        &FailAt::new(vec![TransactionFaultPoint::BeforeCleanup]),
    )
    .expect("installed snapshot");
    assert!(result.cleanup_pending());
    drop(plan);
    let recovered =
        recover_transaction(workspace.path(), &transaction, binding()).expect("restart");
    assert_eq!(recovered.state(), RecoveryState::AlreadyApplied);
    assert_eq!(std::fs::read(workspace.path().join("large")).expect("large"), expected);
    for index in 0..1_024 {
        assert_eq!(
            std::fs::read(workspace.path().join(format!("small-{index:04}"))).expect("small"),
            b"saved"
        );
    }
}

#[test]
fn interrupted_large_snapshot_restores_all_preimages_after_restart() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let (plan, _) = snapshot_plan(workspace.path());
    let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
    let error = apply_with_faults(
        workspace.path(),
        transactions.path(),
        &plan,
        &FailAt::new(vec![
            TransactionFaultPoint::AfterBackupOriginal,
            TransactionFaultPoint::BeforeRollback,
        ]),
    )
    .expect_err("interrupted restore");
    assert_eq!(error.rollback_status(), RollbackStatus::Indeterminate);
    drop(plan);
    let recovered =
        recover_transaction(workspace.path(), &transaction, binding()).expect("restart");
    assert_eq!(recovered.state(), RecoveryState::RolledBackCleanly);
    assert_eq!(
        std::fs::read(workspace.path().join("large")).expect("original"),
        vec![3_u8; 8 * 1024 * 1024 + 1]
    );
    assert_eq!(std::fs::read_dir(workspace.path()).expect("workspace entries").count(), 1);
}

#[test]
fn changed_snapshot_content_is_rejected_before_any_workspace_effect() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let mut file = tempfile::tempfile().expect("file");
    file.write_all(b"drift").expect("changed bytes");
    let operation = PatchOperation::create_snapshot(
        WorkspacePath::new("target").expect("path"),
        SnapshotFile::new(file, peritus_codec::sha256(b"saved"), 5, FileMode::Regular),
    );
    let plan = PatchSet::from_snapshot(
        binding().workspace_id(),
        binding().generation(),
        binding().revision(),
        vec![operation],
    )
    .expect("inert plan")
    .plan(binding().workspace_id(), binding().generation(), binding().revision())
    .expect("bound");
    let error = apply_with_faults(workspace.path(), transactions.path(), &plan, &NoFaults)
        .expect_err("changed snapshot");
    assert_eq!(error.rollback_status(), RollbackStatus::NotRequired);
    assert_eq!(std::fs::read_dir(workspace.path()).expect("entries").count(), 0);
}
