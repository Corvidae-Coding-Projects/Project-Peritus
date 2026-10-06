//! Native path acceptance survives the same publication, rollback and replay boundaries.

use super::*;

fn native_plan(path: &str) -> PatchPlan {
    let identity = binding();
    let operation = PatchOperation::create(
        WorkspacePath::new(path).unwrap(),
        FinalFile::new(b"retained".to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
            .unwrap(),
    );
    PatchSet::new(
        identity.workspace_id(),
        identity.generation(),
        identity.revision(),
        vec![operation],
    )
    .unwrap()
    .plan(identity.workspace_id(), identity.generation(), identity.revision())
    .unwrap()
}

#[test]
fn extended_depth_survives_publication_interruption_and_exact_retry() {
    let workspace = tempfile::tempdir().unwrap();
    let transactions = tempfile::tempdir().unwrap();
    let relative = format!("{}/saved", vec!["d"; 257].join("/"));
    let plan = native_plan(&relative);
    let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
    let error = apply_with_faults(
        workspace.path(),
        transactions.path(),
        &plan,
        &FailAt::new(vec![
            TransactionFaultPoint::AfterCreateDirectory,
            TransactionFaultPoint::BeforeRollback,
        ]),
    )
    .unwrap_err();
    assert_eq!(error.rollback_status(), RollbackStatus::Indeterminate);
    assert_eq!(
        recover_transaction(workspace.path(), &transaction, binding()).unwrap().state(),
        RecoveryState::RolledBackCleanly
    );
    assert!(!workspace.path().join("d").exists());
    let applied = apply_with_faults(
        workspace.path(),
        transactions.path(),
        &plan,
        &FailAt::new(vec![TransactionFaultPoint::BeforeCleanup]),
    )
    .unwrap();
    assert_eq!(
        Manifest::decode(applied.installed_manifest()).unwrap().encode().unwrap(),
        applied.installed_manifest()
    );
    assert_eq!(&applied.installed_manifest()[20..22], &5_u16.to_be_bytes());
    let retry = apply_with_faults(workspace.path(), transactions.path(), &plan, &NoFaults);
    assert!(retry.is_err());
    assert_eq!(
        recover_transaction(workspace.path(), &transaction, binding()).unwrap().state(),
        RecoveryState::AlreadyApplied
    );
    assert_eq!(std::fs::read(workspace.path().join(relative)).unwrap(), b"retained");
    assert_eq!(applied.identity(), plan.identity());
}

#[cfg(unix)]
#[test]
fn native_names_recover_rollback_and_exact_installed_receipt() {
    for name in ["a:b", "a\\b", "name.", "NUL", "line\nbreak"] {
        let workspace = tempfile::tempdir().unwrap();
        let transactions = tempfile::tempdir().unwrap();
        let plan = native_plan(name);
        let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
        let error = apply_with_faults(
            workspace.path(),
            transactions.path(),
            &plan,
            &FailAt::new(vec![TransactionFaultPoint::AfterInstallFinal]),
        )
        .unwrap_err();
        assert_eq!(error.rollback_status(), RollbackStatus::Restored);
        assert!(!workspace.path().join(name).exists());
        let applied = apply_with_faults(
            workspace.path(),
            transactions.path(),
            &plan,
            &FailAt::new(vec![TransactionFaultPoint::BeforeCleanup]),
        )
        .unwrap();
        let receipt = std::fs::read(transaction.join("manifest.bin")).unwrap();
        assert_eq!(receipt, applied.installed_manifest());
        assert_eq!(Manifest::decode(&receipt).unwrap().encode().unwrap(), receipt);
        assert_eq!(&receipt[20..22], &5_u16.to_be_bytes());
        assert_eq!(
            recover_transaction(workspace.path(), &transaction, binding()).unwrap().state(),
            RecoveryState::AlreadyApplied
        );
        assert_eq!(std::fs::read(workspace.path().join(name)).unwrap(), b"retained");
    }
}
