//! Real competing processes exercise shared input, inheritance, retry, and movable ownership.

use super::acl_fixture as fixture;
use peritus_sandbox::{
    FileOperation, FileOperationSet, FilesystemRule, PathScope, RuleEffect, SandboxPath,
};
use peritus_sandbox_windows::{
    AclPlan, PathPolicy, WindowsOperation, WindowsPath, WindowsRecovery, compile_acl_plan,
};
use std::path::{Path, PathBuf};

const CHILD: &str = "PERITUS_ACL_CONCURRENCY_FIXTURE";

fn plan(workspace: &Path, target: &Path, sid: &str, descendants: bool) -> AclPlan {
    let input = WindowsPath::from_os_str(target.as_os_str()).unwrap();
    let extra = FilesystemRule::new(
        RuleEffect::Allow,
        SandboxPath::new(input.to_path_buf().to_str().unwrap()).unwrap(),
        if descendants { PathScope::Descendants } else { PathScope::Exact },
        FileOperationSet::from_operations([FileOperation::Read]),
    )
    .unwrap();
    let policy =
        PathPolicy::new(WindowsPath::from_os_str(workspace.as_os_str()).unwrap(), Vec::new())
            .unwrap()
            .with_read_only_inputs(vec![input])
            .unwrap();
    compile_acl_plan(&crate::support::checked_plan(vec![extra]), &policy, sid).unwrap()
}

fn workspace(base: &Path, name: &str) -> PathBuf {
    let path = base.join(name);
    std::fs::create_dir_all(path.join("bin")).unwrap();
    std::fs::create_dir(path.join("workspace")).unwrap();
    std::fs::write(path.join("bin/tool"), b"tool").unwrap();
    path
}

fn child(base: &Path, target: &Path, mode: &str) {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "acl_concurrency::native_acl_cross_process_child", "--nocapture"])
        .env(CHILD, base)
        .env("PERITUS_ACL_TARGET", target)
        .env("PERITUS_ACL_MODE", mode)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child {mode}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("ACL_CHILD_VERIFIED"));
}

fn assert_busy(plan: &AclPlan, backup: &Path, target: &Path) {
    let live = fixture::snapshot(target);
    let error = plan.install(backup).unwrap_err();
    assert_eq!(error.operation(), WindowsOperation::InstallAcl);
    assert_eq!(error.recovery(), WindowsRecovery::Replan);
    assert_eq!(fixture::snapshot(target), live, "busy admission changed another owner's ACL");
    assert!(!backup.exists() || std::fs::read_dir(backup).unwrap().next().is_none());
}

#[test]
fn native_acl_cross_process_child() {
    let Some(base) = std::env::var_os(CHILD) else {
        return;
    };
    // This child deliberately bypasses the fixture scheduling gate and uses the real public API.
    let base = PathBuf::from(base);
    let target = PathBuf::from(std::env::var_os("PERITUS_ACL_TARGET").unwrap());
    let mode = std::env::var("PERITUS_ACL_MODE").unwrap();
    let plan = plan(&base.join("second"), &target, "S-1-5-11", target.is_dir());
    let backup = base.join("second-backup");
    if mode == "busy" {
        assert_busy(&plan, &backup, &target);
    } else if mode == "release" {
        let before = fixture::snapshot(&target);
        let transaction = plan.install(&backup).unwrap();
        std::thread::spawn(move || drop(transaction)).join().unwrap();
        assert_eq!(fixture::snapshot(&target), before);
        assert_eq!(std::fs::read_dir(&backup).unwrap().count(), 0);
    } else {
        assert_eq!(mode, "quarantine");
        let mut transaction = plan.install(&backup).unwrap();
        let protected = target.join("protected-residue");
        std::fs::write(&protected, b"new").unwrap();
        fixture::set(&protected, "D:P(A;;FR;;;AU)(A;;FA;;;WD)");
        assert!(transaction.restore().is_err());
        std::thread::spawn(move || drop(transaction)).join().unwrap();
        let live = fixture::snapshot(&target);
        let error = plan.install(&base.join("quarantine-backup")).unwrap_err();
        assert_eq!(error.recovery(), WindowsRecovery::Replan);
        assert_eq!(fixture::snapshot(&target), live);
        // Resolve this fixture residue for its parent's filesystem cleanup. The child's
        // process-lifetime quarantine deliberately remains until this process exits.
        std::fs::remove_file(protected).unwrap();
    }
    println!("ACL_CHILD_VERIFIED {mode}");
}

#[test]
fn native_acl_volume_exclusion_precedes_snapshot_and_survives_retry_and_thread_moves() {
    let _serial = fixture::serial();
    let root = tempfile::tempdir().unwrap();
    let base = WindowsPath::from_canonicalized(&std::fs::canonicalize(root.path()).unwrap())
        .unwrap()
        .to_path_buf();
    let first = workspace(&base, "first");
    workspace(&base, "second");
    let shared = base.join("shared");
    std::fs::create_dir(&shared).unwrap();
    fixture::set(&shared, "D:(A;OICI;FA;;;WD)");
    let executable = shared.join("executable");
    let model = shared.join("model");
    std::fs::write(&executable, b"shared executable").unwrap();
    std::fs::write(&model, b"shared model").unwrap();
    let originals = [&shared, &executable, &model];
    let before = originals.map(|path| fixture::snapshot(path));
    let mut transaction =
        plan(&first, &shared, "S-1-5-32-545", true).install(&base.join("first-backup")).unwrap();
    let live = originals.map(|path| fixture::snapshot(path));
    for target in [&executable, &model, &shared] {
        child(&base, target, "busy");
    }
    let dynamic = shared.join("dynamic");
    std::fs::write(&dynamic, b"new child").unwrap();
    child(&base, &dynamic, "busy");
    assert_eq!(originals.map(|path| fixture::snapshot(path)), live);
    fixture::set(&dynamic, "D:P(A;;FR;;;BU)(A;;FA;;;WD)");
    assert!(transaction.restore().is_err());
    assert_eq!(transaction.cleanup_state(), peritus_sandbox_windows::CleanupState::RetryRequired);
    child(&base, &model, "busy");
    std::fs::remove_file(dynamic).unwrap();
    std::thread::spawn(move || {
        transaction.restore().unwrap();
        assert!(transaction.restored());
    })
    .join()
    .unwrap();
    assert_eq!(originals.map(|path| fixture::snapshot(path)), before);
    child(&base, &model, "release");
    child(&base, &shared, "quarantine");
    child(&base, &model, "release");
}
