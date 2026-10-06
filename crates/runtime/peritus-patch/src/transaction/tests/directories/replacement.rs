//! Exact type and permission restoration uses one staged replacement and reversible backup.

use super::*;

fn replacement_plan(workspace: &Path, from_directory: bool, to_directory: bool) -> PatchPlan {
    let target = workspace.join("node");
    let before = if from_directory {
        fs::create_dir(&target).unwrap();
        set_mode(&target);
        Preimage::EmptyDirectory { mode: mode() }
    } else {
        fs::write(&target, b"before").unwrap();
        Preimage::from_bytes(b"before", FileMode::Regular)
    };
    let operation = if to_directory {
        PatchOperation::replace_directory(
            WorkspacePath::new("node").unwrap(),
            before,
            DirectoryMode::new(0o777).unwrap(),
        )
        .unwrap()
    } else {
        PatchOperation::replace(
            WorkspacePath::new("node").unwrap(),
            before,
            FinalFile::new(b"after".to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
                .unwrap(),
        )
        .unwrap()
    };
    PatchSet::from_snapshot(
        binding().workspace_id(),
        binding().generation(),
        binding().revision(),
        vec![
            operation,
            PatchOperation::create(
                WorkspacePath::new("z-later").unwrap(),
                FinalFile::new(b"later".to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
                    .unwrap(),
            ),
        ],
    )
    .unwrap()
    .plan(binding().workspace_id(), binding().generation(), binding().revision())
    .unwrap()
}

#[test]
fn typed_replacements_install_and_cold_recover_with_exact_postimage_permissions() {
    for (from_directory, to_directory) in [(true, true), (true, false), (false, true)] {
        let workspace = tempfile::tempdir().unwrap();
        let transactions = tempfile::tempdir().unwrap();
        let plan = replacement_plan(workspace.path(), from_directory, to_directory);
        let expected = plan.operations()[0].postimage();
        let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
        apply_with_faults(
            workspace.path(),
            transactions.path(),
            &plan,
            &FailAt::new(vec![TransactionFaultPoint::BeforeCleanup]),
        )
        .unwrap();
        drop(plan);
        assert_eq!(
            recover_transaction(workspace.path(), transaction, binding()).unwrap().state(),
            RecoveryState::AlreadyApplied
        );
        let observed = observe_absolute(
            &workspace.path().join("node"),
            crate::PatchOperationContext::VerifyResult,
            RollbackStatus::NotRequired,
        )
        .unwrap();
        assert!(observation_matches(
            observed,
            super::super::super::manifest::TargetIdentity::from_preimage(expected)
        ));
    }
}

#[test]
fn interrupted_typed_replacements_restore_original_node_before_and_after_final_install() {
    for (from_directory, to_directory) in [(true, true), (true, false), (false, true)] {
        for point in
            [TransactionFaultPoint::AfterBackupOriginal, TransactionFaultPoint::AfterInstallFinal]
        {
            let workspace = tempfile::tempdir().unwrap();
            let transactions = tempfile::tempdir().unwrap();
            let plan = replacement_plan(workspace.path(), from_directory, to_directory);
            let expected = plan.operations()[0].preimage();
            let transaction = transactions.path().join(format!("txn-{}", plan.identity()));
            let error = apply_with_faults(
                workspace.path(),
                transactions.path(),
                &plan,
                &FailAt::new(vec![point, TransactionFaultPoint::BeforeRollback]),
            )
            .unwrap_err();
            assert_eq!(error.rollback_status(), RollbackStatus::Indeterminate);
            drop(plan);
            assert_eq!(
                recover_transaction(workspace.path(), transaction, binding()).unwrap().state(),
                RecoveryState::RolledBackCleanly
            );
            let observed = observe_absolute(
                &workspace.path().join("node"),
                crate::PatchOperationContext::VerifyResult,
                RollbackStatus::NotRequired,
            )
            .unwrap();
            assert!(observation_matches(
                observed,
                super::super::super::manifest::TargetIdentity::from_preimage(expected)
            ));
        }
    }
}
