use std::{cell::RefCell, io};

use peritus_types::{Generation, RevisionNumber, WorkspaceId};

use crate::{
    FileMode, FinalFile, LineEndingPolicy, PatchOperation, PatchPlan, PatchSet, Preimage,
    RecoveryBinding, RecoveryState, RollbackStatus, TransactionFaultPoint, WorkspacePath,
};

use super::{
    FaultInjector, NoFaults,
    apply::apply_with_faults,
    filesystem::{
        Observation, discover_missing_directories, observation_matches, observe_absolute,
    },
    manifest::Manifest,
    recover::recover_transaction,
    roots::prepare_roots,
    storage::prepare_transaction,
};

struct FailAt {
    remaining: RefCell<Vec<TransactionFaultPoint>>,
}

impl FailAt {
    fn new(points: Vec<TransactionFaultPoint>) -> Self {
        Self { remaining: RefCell::new(points) }
    }
}

impl FaultInjector for FailAt {
    fn check(&self, point: TransactionFaultPoint) -> io::Result<()> {
        let mut remaining = self.remaining.borrow_mut();
        if remaining.first() == Some(&point) {
            remaining.remove(0);
            Err(io::Error::other("injected transaction fault"))
        } else {
            Ok(())
        }
    }
}

fn plan() -> PatchPlan {
    let replace = PatchOperation::replace(
        WorkspacePath::new("old").expect("path"),
        Preimage::from_bytes(b"before", FileMode::Regular),
        FinalFile::new(b"after".to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
            .expect("final"),
    )
    .expect("replace");
    let create = PatchOperation::create(
        WorkspacePath::new("new").expect("path"),
        FinalFile::new(b"new".to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
            .expect("final"),
    );
    let workspace_id = WorkspaceId::new([3; 16]).expect("workspace id");
    PatchSet::new(workspace_id, Generation::first(), RevisionNumber::first(), vec![replace, create])
        .expect("patch")
        .plan(workspace_id, Generation::first(), RevisionNumber::first())
        .expect("plan")
}

fn binding() -> RecoveryBinding {
    RecoveryBinding::new(
        WorkspaceId::new([3; 16]).expect("workspace id"),
        Generation::first(),
        RevisionNumber::first(),
    )
}

fn nested_plan() -> PatchPlan {
    let operation = PatchOperation::create(
        WorkspacePath::new("nested/file").expect("path"),
        FinalFile::new(b"new".to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
            .expect("final"),
    );
    let workspace_id = binding().workspace_id();
    PatchSet::new(workspace_id, binding().generation(), binding().revision(), vec![operation])
        .expect("patch")
        .plan(workspace_id, binding().generation(), binding().revision())
        .expect("plan")
}

#[test]
fn failure_after_first_install_rolls_back_all_targets() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    std::fs::write(workspace.path().join("old"), b"before").expect("original");
    let fault = FailAt::new(vec![TransactionFaultPoint::AfterInstallFinal]);
    let error = apply_with_faults(workspace.path(), transactions.path(), &plan(), &fault)
        .expect_err("fault");
    assert_eq!(error.rollback_status(), RollbackStatus::Restored);
    assert_eq!(std::fs::read(workspace.path().join("old")).expect("old"), b"before");
    assert!(!workspace.path().join("new").exists());
}

#[test]
fn restart_recovers_a_partial_install_to_all_preimages() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    std::fs::write(workspace.path().join("old"), b"before").expect("original");
    let plan = plan();
    let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
    let fault = FailAt::new(vec![
        TransactionFaultPoint::AfterBackupOriginal,
        TransactionFaultPoint::BeforeRollback,
    ]);
    let error = apply_with_faults(workspace.path(), transactions.path(), &plan, &fault)
        .expect_err("crash-visible failure");
    assert_eq!(error.rollback_status(), RollbackStatus::Indeterminate);
    let recovered =
        recover_transaction(workspace.path(), &transaction, binding()).expect("recover");
    assert_eq!(recovered.state(), RecoveryState::RolledBackCleanly);
    assert_eq!(std::fs::read(workspace.path().join("old")).expect("old"), b"before");
    assert!(!workspace.path().join("new").exists());
}

#[test]
fn restart_recognizes_a_fully_installed_transaction() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    std::fs::write(workspace.path().join("old"), b"before").expect("original");
    let plan = plan();
    let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
    let fault = FailAt::new(vec![TransactionFaultPoint::BeforeCleanup]);
    let applied = apply_with_faults(workspace.path(), transactions.path(), &plan, &fault)
        .expect("durably applied");
    assert!(applied.cleanup_pending());
    let recovered =
        recover_transaction(workspace.path(), &transaction, binding()).expect("recover");
    assert_eq!(recovered.state(), RecoveryState::AlreadyApplied);
    assert!(!recovered.cleanup_pending());
    assert_eq!(std::fs::read(workspace.path().join("old")).expect("old"), b"after");
    assert_eq!(std::fs::read(workspace.path().join("new")).expect("new"), b"new");
}

#[test]
fn prepared_transaction_recovers_without_touching_workspace() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    std::fs::write(workspace.path().join("old"), b"before").expect("original");
    let plan = plan();
    let roots = prepare_roots(workspace.path(), transactions.path()).expect("roots");
    let transaction = roots.transaction_root.join(format!("txn-{}", plan.identity()));
    std::fs::create_dir(&transaction).expect("transaction");
    let directories = discover_missing_directories(
        &roots.workspace,
        plan.operations().iter().map(|operation| operation.path().clone()),
    )
    .expect("directories");
    let manifest = Manifest::from_plan(&plan, directories);
    prepare_transaction(&transaction, &plan, &manifest, &NoFaults).expect("prepared");
    let recovered =
        recover_transaction(workspace.path(), &transaction, binding()).expect("recover");
    assert_eq!(recovered.state(), RecoveryState::RolledBackCleanly);
    assert_eq!(std::fs::read(workspace.path().join("old")).expect("old"), b"before");
    assert!(!workspace.path().join("new").exists());
}

#[test]
fn corrupt_manifest_is_quarantined_and_never_success() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let transaction = transactions.path().join("txn-corrupt");
    std::fs::create_dir(&transaction).expect("transaction");
    std::fs::write(transaction.join("manifest.bin"), b"not a manifest").expect("manifest");
    let recovered =
        recover_transaction(workspace.path(), &transaction, binding()).expect("recover");
    assert_eq!(recovered.state(), RecoveryState::Indeterminate);
    assert!(recovered.quarantined());
    assert!(!transaction.exists());
}

#[test]
fn parseable_same_length_manifest_tamper_is_quarantined_without_workspace_effect() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    std::fs::write(workspace.path().join("old"), b"before").expect("original");
    let plan = plan();
    let roots = prepare_roots(workspace.path(), transactions.path()).expect("roots");
    let transaction = roots.transaction_root.join(format!("txn-{}", plan.identity()));
    std::fs::create_dir(&transaction).expect("transaction");
    let directories = discover_missing_directories(
        &roots.workspace,
        plan.operations().iter().map(|operation| operation.path().clone()),
    )
    .expect("directories");
    let manifest = Manifest::from_plan(&plan, directories);
    prepare_transaction(&transaction, &plan, &manifest, &NoFaults).expect("prepared");
    let manifest_path = transaction.join(super::storage::MANIFEST_FILE);
    let mut bytes = std::fs::read(&manifest_path).expect("read manifest");
    let path_offset =
        bytes.windows(3).position(|window| window == b"old").expect("encoded target path");
    bytes[path_offset] = b'x';
    std::fs::write(&manifest_path, bytes).expect("tamper manifest");

    let recovered =
        recover_transaction(workspace.path(), &transaction, binding()).expect("recover tamper");
    assert_eq!(recovered.state(), RecoveryState::Indeterminate);
    assert!(recovered.quarantined());
    assert_eq!(std::fs::read(workspace.path().join("old")).expect("original"), b"before");
    assert!(!workspace.path().join("xld").exists());
}

#[test]
fn oversized_observation_never_matches_a_forged_empty_digest() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("large");
    std::fs::write(&path, vec![7; crate::set::MAX_FILE_BYTES + 1]).expect("large file");
    let observed = observe_absolute(
        &path,
        crate::PatchOperationContext::InspectPreimage,
        RollbackStatus::NotRequired,
    )
    .expect("observation");
    assert_eq!(observed, Observation::Oversized);
    let forged = super::manifest::FileIdentity {
        digest: peritus_codec::sha256(&[]),
        size: (crate::set::MAX_FILE_BYTES + 1) as u64,
        mode: FileMode::Regular,
    };
    assert!(!observation_matches(observed, Some(forged)));
}

#[test]
fn failure_after_directory_creation_restores_absence() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let fault = FailAt::new(vec![TransactionFaultPoint::AfterCreateDirectory]);
    let error = apply_with_faults(workspace.path(), transactions.path(), &nested_plan(), &fault)
        .expect_err("directory fault");
    assert_eq!(error.rollback_status(), RollbackStatus::Restored);
    assert!(!workspace.path().join("nested").exists());
}

#[test]
fn restart_removes_directory_created_before_first_target_effect() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let plan = nested_plan();
    let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
    let fault = FailAt::new(vec![
        TransactionFaultPoint::AfterCreateDirectory,
        TransactionFaultPoint::BeforeRollback,
    ]);
    let error = apply_with_faults(workspace.path(), transactions.path(), &plan, &fault)
        .expect_err("restart-visible directory");
    assert_eq!(error.rollback_status(), RollbackStatus::Indeterminate);
    assert!(workspace.path().join("nested").is_dir());
    let recovered =
        recover_transaction(workspace.path(), &transaction, binding()).expect("recover");
    assert_eq!(recovered.state(), RecoveryState::RolledBackCleanly);
    assert!(!workspace.path().join("nested").exists());
}

#[test]
fn nonempty_created_directory_is_indeterminate_not_restored() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let plan = nested_plan();
    let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
    let fault = FailAt::new(vec![
        TransactionFaultPoint::AfterCreateDirectory,
        TransactionFaultPoint::BeforeRollback,
    ]);
    apply_with_faults(workspace.path(), transactions.path(), &plan, &fault)
        .expect_err("restart-visible directory");
    std::fs::write(workspace.path().join("nested/unrelated"), b"external").expect("drift");
    let recovered =
        recover_transaction(workspace.path(), &transaction, binding()).expect("recover");
    assert_eq!(recovered.state(), RecoveryState::Indeterminate);
    assert!(transaction.exists());
    assert_eq!(
        std::fs::read(workspace.path().join("nested/unrelated")).expect("external"),
        b"external"
    );
}

#[test]
fn binding_mismatch_is_reported_without_effects() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    std::fs::write(workspace.path().join("old"), b"before").expect("original");
    let plan = plan();
    let roots = prepare_roots(workspace.path(), transactions.path()).expect("roots");
    let transaction = roots.transaction_root.join(format!("txn-{}", plan.identity()));
    std::fs::create_dir(&transaction).expect("transaction");
    let directories = discover_missing_directories(
        &roots.workspace,
        plan.operations().iter().map(|operation| operation.path().clone()),
    )
    .expect("directories");
    let manifest = Manifest::from_plan(&plan, directories);
    prepare_transaction(&transaction, &plan, &manifest, &NoFaults).expect("prepared");
    let different = RecoveryBinding::new(
        WorkspaceId::new([4; 16]).expect("other workspace"),
        binding().generation(),
        binding().revision(),
    );
    let recovered =
        recover_transaction(workspace.path(), &transaction, different).expect("inspect mismatch");
    assert_eq!(recovered.state(), RecoveryState::Indeterminate);
    assert_eq!(recovered.binding(), Some(binding()));
    assert!(transaction.exists());
    assert_eq!(std::fs::read(workspace.path().join("old")).expect("old"), b"before");
}
