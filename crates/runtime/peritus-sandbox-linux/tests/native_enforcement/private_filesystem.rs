//! Real private-root projection denies ambient files while admitting exact installed inputs.

use super::{native_support::*, support};
use peritus_sandbox::{
    FileOperation, FileOperationSet, FilesystemRule, PathScope, RuleEffect, SandboxPath,
};
use peritus_sandbox_linux::{MountPlan, MountPolicy, TargetCommand};
use std::io::Write as _;
use std::path::{Path, PathBuf};

#[test]
fn private_root_admits_installed_inputs_but_hides_unrelated_host_files() {
    let _guard = native_test_guard();
    if !native_sandbox_available() {
        std::io::stderr().write_all(b"native private-filesystem test unavailable: required Linux namespace/helper controls absent\n").unwrap();
        return;
    }
    let workspace = tempfile::tempdir().unwrap();
    for name in [".git", ".peritus", ".crosslink"] {
        std::fs::create_dir(workspace.path().join(name)).unwrap();
    }
    std::fs::write(workspace.path().join("input.txt"), b"input").unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let weights = inputs.path().join("weights.bin");
    let ambient = inputs.path().join("ambient-credential.txt");
    std::fs::write(&weights, b"installed local input").unwrap();
    std::fs::write(&ambient, b"private fixture - never admitted").unwrap();
    let executable = PathBuf::from("/usr/bin/cat").canonicalize().unwrap();
    let mut rules = vec![read_rule(&executable, true), read_rule(&weights, false)];
    for root in ["/usr/lib", "/usr/lib64", "/lib", "/lib64"] {
        if let Ok(root) = Path::new(root).canonicalize() {
            rules.push(read_rule(&root, true));
        }
    }
    let checked = support::checked_private_files_plan(workspace.path(), rules);
    let policy = MountPolicy::new(workspace.path(), Vec::new())
        .unwrap()
        .with_private_filesystem(helper_path().canonicalize().unwrap())
        .unwrap();
    let mounts = MountPlan::project(&checked, &policy).unwrap();
    let target = |file: &Path| {
        manifest(
            TargetCommand::new(
                executable.to_string_lossy().into_owned(),
                vec![file.to_string_lossy().into_owned()],
            )
            .unwrap(),
            workspace.path(),
            mounts.landlock_rules().to_vec(),
        )
    };
    let allowed = run_bubblewrapped(&mounts, &target(&weights));
    assert!(allowed.status.success(), "{}", allowed.stderr);
    assert_eq!(allowed.target_stdout, b"installed local input");
    let denied = run_bubblewrapped(&mounts, &target(&ambient));
    assert!(!denied.status.success());
    assert!(denied.target_stdout.is_empty());
}

fn read_rule(path: &Path, execute: bool) -> FilesystemRule {
    let mut operations = FileOperationSet::from_operations([
        FileOperation::Read,
        FileOperation::Discover,
        FileOperation::Metadata,
    ]);
    if execute {
        operations.insert(FileOperation::Execute);
    }
    FilesystemRule::new(
        RuleEffect::Allow,
        SandboxPath::new(path.to_string_lossy().into_owned()).unwrap(),
        if path.is_dir() { PathScope::Descendants } else { PathScope::Exact },
        operations,
    )
    .unwrap()
}
