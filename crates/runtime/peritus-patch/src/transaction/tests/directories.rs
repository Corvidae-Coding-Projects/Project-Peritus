//! Empty-directory effects share the file transaction and its restart frontier.

use super::*;
use crate::{DirectoryMode, ErrorCode};
use std::{fs, path::Path};
mod replacement;

#[cfg(unix)]
fn mode() -> DirectoryMode {
    DirectoryMode::new(0o750).expect("directory mode")
}

#[cfg(not(unix))]
fn mode() -> DirectoryMode {
    DirectoryMode::new(0o777).expect("directory mode")
}

fn set_mode(path: &Path) {
    super::super::filesystem::set_directory_mode(path, mode()).expect("directory permissions");
}

fn mixed_plan(workspace: &Path) -> PatchPlan {
    fs::create_dir(workspace.join("a-removed")).expect("empty directory");
    set_mode(&workspace.join("a-removed"));
    fs::write(workspace.join("c-file"), b"before").expect("file");
    let operations = vec![
        PatchOperation::delete_directory(WorkspacePath::new("a-removed").expect("path"), mode()),
        PatchOperation::create_directory(WorkspacePath::new("b-created").expect("path"), mode()),
        PatchOperation::replace(
            WorkspacePath::new("c-file").expect("path"),
            Preimage::from_bytes(b"before", FileMode::Regular),
            FinalFile::new(b"after".to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
                .expect("file"),
        )
        .expect("replace"),
    ];
    PatchSet::from_snapshot(
        binding().workspace_id(),
        binding().generation(),
        binding().revision(),
        operations,
    )
    .expect("one mixed transaction")
    .plan(binding().workspace_id(), binding().generation(), binding().revision())
    .expect("bound plan")
}

#[test]
fn mixed_files_and_empty_directories_install_and_reopen_as_one_transaction() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let plan = mixed_plan(workspace.path());
    let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
    let result = apply_with_faults(
        workspace.path(),
        transactions.path(),
        &plan,
        &FailAt::new(vec![TransactionFaultPoint::BeforeCleanup]),
    )
    .expect("installed");
    assert!(result.cleanup_pending());
    drop(plan);
    assert_eq!(
        recover_transaction(workspace.path(), &transaction, binding()).expect("reopen").state(),
        RecoveryState::AlreadyApplied
    );
    assert!(!workspace.path().join("a-removed").exists());
    assert_eq!(fs::read(workspace.path().join("c-file")).expect("file"), b"after");
    assert_eq!(
        observe_absolute(
            &workspace.path().join("b-created"),
            crate::PatchOperationContext::VerifyResult,
            RollbackStatus::NotRequired
        )
        .expect("directory"),
        Observation::Present(super::super::manifest::TargetIdentity::EmptyDirectory {
            mode: mode()
        })
    );
}

#[test]
fn interrupted_mixed_transaction_restores_directory_identity_and_all_file_preimages() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let plan = mixed_plan(workspace.path());
    #[cfg(unix)]
    let original_inode = {
        use std::os::unix::fs::MetadataExt as _;
        fs::metadata(workspace.path().join("a-removed")).expect("original").ino()
    };
    let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
    let error = apply_with_faults(
        workspace.path(),
        transactions.path(),
        &plan,
        &FailAt::new(vec![
            TransactionFaultPoint::AfterInstallFinal,
            TransactionFaultPoint::BeforeRollback,
        ]),
    )
    .expect_err("interrupted");
    assert_eq!(error.rollback_status(), RollbackStatus::Indeterminate);
    drop(plan);
    assert_eq!(
        recover_transaction(workspace.path(), &transaction, binding()).expect("reopen").state(),
        RecoveryState::RolledBackCleanly
    );
    assert!(!workspace.path().join("b-created").exists());
    assert_eq!(fs::read(workspace.path().join("c-file")).expect("file"), b"before");
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
        let restored =
            fs::metadata(workspace.path().join("a-removed")).expect("restored directory");
        assert_eq!(restored.ino(), original_inode);
        assert_eq!(restored.permissions().mode() & 0o7777, u32::from(mode().bits()));
    }
}

#[test]
fn nonempty_directory_rejects_before_any_other_target_changes() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let plan = mixed_plan(workspace.path());
    fs::write(workspace.path().join("a-removed/external"), b"keep").expect("external child");
    let error = apply_with_faults(workspace.path(), transactions.path(), &plan, &NoFaults)
        .expect_err("nonempty");
    assert_eq!(error.code(), ErrorCode::UnsafeFilesystemTarget);
    assert_eq!(error.rollback_status(), RollbackStatus::NotRequired);
    assert_eq!(fs::read(workspace.path().join("c-file")).expect("file"), b"before");
    assert!(!workspace.path().join("b-created").exists());
    assert_eq!(fs::read(workspace.path().join("a-removed/external")).expect("child"), b"keep");
}

struct AddBackupChild<'a>(&'a Path);
impl FaultInjector for AddBackupChild<'_> {
    fn check(&self, point: TransactionFaultPoint) -> io::Result<()> {
        if point == TransactionFaultPoint::AfterBackupOriginal {
            fs::write(self.0.join("backup-0000/external"), b"keep")?;
        }
        Ok(())
    }
}

#[test]
fn child_added_to_a_directory_backup_is_preserved_through_failed_apply_and_recovery() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let plan = mixed_plan(workspace.path());
    let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
    let error = apply_with_faults(
        workspace.path(),
        transactions.path(),
        &plan,
        &AddBackupChild(&transaction),
    )
    .expect_err("backup changed");
    assert_eq!(error.rollback_status(), RollbackStatus::Indeterminate);
    assert_eq!(
        recover_transaction(workspace.path(), &transaction, binding()).expect("reopen").state(),
        RecoveryState::Indeterminate
    );
    assert_eq!(
        fs::read(transaction.join("backup-0000/external")).expect("external child preserved"),
        b"keep"
    );
    assert_eq!(fs::read(workspace.path().join("c-file")).expect("file"), b"before");
}

#[test]
fn cleanup_never_recursively_removes_children_added_after_installation() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let plan = mixed_plan(workspace.path());
    let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
    apply_with_faults(
        workspace.path(),
        transactions.path(),
        &plan,
        &FailAt::new(vec![TransactionFaultPoint::BeforeCleanup]),
    )
    .expect("installed");
    fs::write(transaction.join("backup-0000/external"), b"keep").expect("external child");
    assert!(super::super::storage::cleanup_transaction(&transaction, transactions.path()).is_err());
    assert!(transaction.join("manifest.bin").is_file());
    assert_eq!(fs::read(transaction.join("backup-0000/external")).expect("child"), b"keep");
}

#[cfg(unix)]
#[test]
fn changed_directory_permissions_are_a_preimage_conflict_without_effects() {
    use std::os::unix::fs::PermissionsExt as _;
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let plan = mixed_plan(workspace.path());
    fs::set_permissions(workspace.path().join("a-removed"), fs::Permissions::from_mode(0o700))
        .expect("external permissions");
    let error = apply_with_faults(workspace.path(), transactions.path(), &plan, &NoFaults)
        .expect_err("permission conflict");
    assert_eq!(error.code(), ErrorCode::PreimageMismatch);
    assert_eq!(error.rollback_status(), RollbackStatus::NotRequired);
    assert_eq!(
        fs::metadata(workspace.path().join("a-removed")).expect("directory").permissions().mode()
            & 0o7777,
        0o700
    );
    assert_eq!(fs::read(workspace.path().join("c-file")).expect("file"), b"before");
}

#[test]
fn child_added_to_an_installed_directory_is_not_removed_during_recovery() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let plan = mixed_plan(workspace.path());
    let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
    apply_with_faults(
        workspace.path(),
        transactions.path(),
        &plan,
        &FailAt::new(vec![TransactionFaultPoint::BeforeCleanup]),
    )
    .expect("installed");
    fs::write(workspace.path().join("b-created/external"), b"keep").expect("external child");
    assert_eq!(
        recover_transaction(workspace.path(), &transaction, binding()).expect("reopen").state(),
        RecoveryState::Indeterminate
    );
    assert_eq!(fs::read(workspace.path().join("b-created/external")).expect("child"), b"keep");
}

#[test]
fn directory_manifest_keeps_large_file_and_many_path_snapshot_support() {
    use std::io::Write as _;
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let bytes = vec![37; 8 * 1024 * 1024 + 1];
    let mut file = tempfile::tempfile().expect("snapshot source");
    file.write_all(&bytes).expect("snapshot bytes");
    let mut operations = vec![PatchOperation::create_snapshot(
        WorkspacePath::new("large").expect("path"),
        crate::SnapshotFile::new(
            file,
            peritus_codec::sha256(&bytes),
            bytes.len() as u64,
            FileMode::Regular,
        ),
    )];
    for index in 0..=1_024 {
        operations.push(PatchOperation::create_directory(
            WorkspacePath::new(format!("empty-{index:04}")).expect("path"),
            mode(),
        ));
    }
    let plan = PatchSet::from_snapshot(
        binding().workspace_id(),
        binding().generation(),
        binding().revision(),
        operations,
    )
    .expect("snapshot beyond inline bounds")
    .plan(binding().workspace_id(), binding().generation(), binding().revision())
    .expect("plan");
    let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
    apply_with_faults(
        workspace.path(),
        transactions.path(),
        &plan,
        &FailAt::new(vec![TransactionFaultPoint::BeforeCleanup]),
    )
    .expect("installed");
    drop(plan);
    assert_eq!(
        recover_transaction(workspace.path(), &transaction, binding()).expect("reopen").state(),
        RecoveryState::AlreadyApplied
    );
    assert_eq!(fs::read(workspace.path().join("large")).expect("large file"), bytes);
    assert_eq!(fs::read_dir(workspace.path()).expect("workspace entries").count(), 1_026);
}
