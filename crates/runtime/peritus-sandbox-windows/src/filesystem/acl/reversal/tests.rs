//! Native transaction retry preserves exact originals and backup ownership.

use crate::{channels::test_support as support, native::acl::test_fixture as fixture};

use crate::{CleanupState, PathPolicy, WindowsPath, compile_acl_plan};

#[test]
fn failed_exact_verification_retains_all_backups_and_originals_for_retry() {
    let _serial = fixture::serial();
    let root = tempfile::tempdir().unwrap();
    let base = WindowsPath::from_canonicalized(&std::fs::canonicalize(root.path()).unwrap())
        .unwrap()
        .to_path_buf();
    let workspace = base.join("workspace");
    std::fs::create_dir_all(workspace.join("bin")).unwrap();
    std::fs::create_dir(workspace.join("workspace")).unwrap();
    std::fs::write(workspace.join("bin/tool"), b"fixture").unwrap();
    std::fs::write(workspace.join("workspace/child"), b"child").unwrap();
    let target = workspace.join("workspace");
    fixture::set(&target, "D:(A;OICI;FA;;;WD)");
    let before = fixture::snapshot(&target);
    assert_eq!(fixture::control(&before) & 0x1400, 0);
    assert!(fixture::sddl(&target).starts_with("D:"));
    let policy = PathPolicy::new(
        WindowsPath::from_os_str(workspace.as_os_str()).unwrap(),
        vec![WindowsPath::from_os_str(target.join("private").as_os_str()).unwrap()],
    )
    .unwrap();
    let plan = compile_acl_plan(&support::checked_plan(Vec::new()), &policy, "S-1-1-0").unwrap();
    let backup = base.join("backup");
    let mut transaction = plan.install(&backup).unwrap();
    let competing = plan.install(&base.join("competing-backup")).unwrap_err();
    assert_eq!(competing.recovery(), crate::WindowsRecovery::Replan);
    assert!(!base.join("competing-backup").exists());
    let owned = transaction.backup_directory.clone().unwrap();
    let backup_count = std::fs::read_dir(&owned).unwrap().count();
    assert_eq!(backup_count, transaction.pending_reversal_count());
    transaction.originals.get(0).fail_next_verification();
    transaction.originals.get(0).fail_next_inheritance();
    let failure = transaction.restore().unwrap_err();
    assert_eq!(failure.detail(), "injected inheritance restore failure");
    assert_eq!(transaction.cleanup_state(), CleanupState::RetryRequired);
    assert!(!transaction.restored());
    assert_eq!(
        fixture::snapshot(&target),
        before,
        "one failure skipped independent exact restoration"
    );
    assert_eq!(
        std::fs::read_dir(&owned).unwrap().count(),
        backup_count,
        "backup released before verification"
    );
    assert_eq!(transaction.pending_reversal_count(), backup_count);
    transaction.restore().unwrap();
    assert!(transaction.restored());
    assert_eq!(fixture::snapshot(&target), before);
    assert!(!owned.exists());
    assert!(!target.join("private").exists());
}
