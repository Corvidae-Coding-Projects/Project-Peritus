use super::*;

#[test]
fn head_only_discard_preserves_detached_commits_and_restores_parent_staging() {
    let root = repository();
    let nested = root.path().join("nested");
    fs::rename(repository().keep(), &nested).unwrap();
    git(root.path(), &["add", "nested"], None).unwrap();
    let index = git(root.path(), &["ls-files", "--stage", "-z"], None).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    git(&nested, &["checkout", "--detach"], None).unwrap();
    git(&nested, &["commit", "--allow-empty", "-qm", "detached task commit"], None).unwrap();
    let task_commit = text(git(&nested, &["rev-parse", "HEAD"], None).unwrap()).unwrap();
    git(root.path(), &["add", "nested"], None).unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    assert_eq!(paths, vec![PathBuf::from("nested")]);
    let recovered = baseline.discard(root.path(), &paths).unwrap();
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(recovered[0].join("head.json")).unwrap()).unwrap();
    let reference = record["retained_commit_ref"].as_str().unwrap();
    assert_eq!(text(git(&nested, &["rev-parse", reference], None).unwrap()).unwrap(), task_commit);
    assert_eq!(git(root.path(), &["ls-files", "--stage", "-z"], None).unwrap(), index);
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
    assert!(baseline.discard(root.path(), &paths).unwrap().is_empty());
}

#[test]
fn baseline_commit_is_pinned_in_its_own_repository_and_missing_commit_preflights() {
    let root = repository();
    let nested = root.path().join("nested");
    fs::rename(repository().keep(), &nested).unwrap();
    let mut baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    let commit = baseline.entries["nested"].object.clone();
    assert_eq!(
        text(
            git(&nested, &["rev-parse", &format!("refs/peritus/task-baselines/{commit}")], None)
                .unwrap()
        )
        .unwrap(),
        commit
    );
    git(&nested, &["commit", "--allow-empty", "-qm", "task"], None).unwrap();
    fs::write(root.path().join("new.txt"), b"preserve until preflight completes\n").unwrap();
    baseline.entries.get_mut("nested").unwrap().object = "1".repeat(40);
    let paths = baseline.changed_paths(root.path()).unwrap();
    assert!(baseline.discard(root.path(), &paths).is_err());
    assert!(root.path().join("new.txt").exists());
    assert!(!nested.join(".git/peritus/discarded").exists());
}

#[test]
fn new_repository_discard_preserves_complete_recovery_copy() {
    let root = repository();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    let child = repository();
    let nested = root.path().join("generated");
    fs::rename(child.path(), &nested).unwrap();
    fs::write(nested.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(nested.join("ignored.txt"), "retain ignored work\n").unwrap();
    fs::write(nested.join("tracked.txt"), "task source\n").unwrap();
    git(root.path(), &["add", "--", "generated"], None).unwrap();
    let head = git(&nested, &["rev-parse", "HEAD"], None).unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    let recovered = baseline.discard(root.path(), &paths).expect("discard new repository");
    assert!(!nested.exists());
    assert_eq!(recovered.len(), 1);
    let saved = recovered[0].join("repository");
    assert_eq!(fs::read(saved.join("tracked.txt")).unwrap(), b"task source\n");
    assert_eq!(fs::read(saved.join("ignored.txt")).unwrap(), b"retain ignored work\n");
    assert_eq!(git(&saved, &["rev-parse", "HEAD"], None).unwrap(), head);
    assert!(recovered[0].join("origin.json").is_file());
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
    assert!(git(root.path(), &["ls-files", "--", "generated"], None).unwrap().is_empty());
    assert!(baseline.discard(root.path(), &paths).unwrap().is_empty());
}

#[test]
fn repository_created_over_existing_directory_restores_original_files() {
    let root = repository();
    let nested = root.path().join("generated");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("draft.txt"), b"user preimage\n").unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    git(&nested, &["init", "--quiet"], None).unwrap();
    fs::write(nested.join("draft.txt"), b"task replacement\n").unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    let recovered = baseline.discard(root.path(), &paths).unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(fs::read(nested.join("draft.txt")).unwrap(), b"user preimage\n");
    assert!(!nested.join(".git").exists());
}

#[test]
fn invalid_preimage_prevents_archiving_and_all_other_restore_effects() {
    let root = repository();
    let mut baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    baseline.entries.get_mut("tracked.txt").unwrap().object = "1".repeat(40);
    fs::write(root.path().join("tracked.txt"), b"task change\n").unwrap();
    let child = repository();
    let nested = root.path().join("generated");
    fs::rename(child.path(), &nested).unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    assert!(baseline.discard(root.path(), &paths).is_err());
    assert!(nested.join(".git").exists());
    assert_eq!(fs::read(root.path().join("tracked.txt")).unwrap(), b"task change\n");
    assert!(!root.path().join(".git/peritus/discarded").exists());
}

#[test]
fn new_repository_inside_existing_nested_repository_is_recoverable() {
    let root = repository();
    let outer = root.path().join("outer");
    fs::rename(repository().keep(), &outer).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::rename(repository().keep(), outer.join("inner")).unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    let recovered = baseline.discard(root.path(), &paths).unwrap();
    assert_eq!(recovered.len(), 1);
    assert!(recovered[0].join("repository/.git").is_dir());
    assert!(!outer.join("inner").exists());
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn replaced_nested_root_is_not_followed_during_discard() {
    let root = repository();
    let outside = tempfile::tempdir().unwrap();
    let nested = root.path().join("nested");
    fs::rename(repository().keep(), &nested).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    let displaced = outside.path().join("displaced");
    fs::rename(&nested, &displaced).unwrap();
    std::os::unix::fs::symlink(&displaced, &nested).unwrap();
    fs::write(root.path().join("new.txt"), b"preserve until preflight completes\n").unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    assert!(baseline.discard(root.path(), &paths).is_err());
    assert!(nested.is_symlink());
    assert!(root.path().join("new.txt").is_file());
}

#[test]
fn linked_worktree_uses_its_actual_git_directory_for_recovery() {
    let source = repository();
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("managed");
    git(source.path(), &["worktree", "add", "--detach", workspace.to_str().unwrap(), "HEAD"], None)
        .unwrap();
    assert!(workspace.join(".git").is_file());
    let baseline = ManagedBaseline::capture(&workspace, true).unwrap();
    fs::rename(repository().keep(), workspace.join("generated")).unwrap();
    let paths = baseline.changed_paths(&workspace).unwrap();
    let recovered = baseline.discard(&workspace, &paths).unwrap();
    let git_directory = PathBuf::from(
        text(git(&workspace, &["rev-parse", "--absolute-git-dir"], None).unwrap()).unwrap(),
    );
    assert_eq!(recovered.len(), 1);
    assert!(
        recovered[0].starts_with(git_directory.canonicalize().unwrap().join("peritus/discarded"))
    );
    assert!(recovered[0].join("repository/tracked.txt").is_file());
    assert!(workspace.join(".git").is_file());
    assert!(baseline.changed_paths(&workspace).unwrap().is_empty());
}
