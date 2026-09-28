use super::*;

#[test]
fn linked_index_lock_preserves_all_source_and_allows_retry() {
    let root = repository();
    let donor = repository();
    let nested = root.path().join("nested");
    git(
        donor.path(),
        &["worktree", "add", "--quiet", "--detach", nested.to_str().unwrap(), "HEAD"],
        None,
    )
    .unwrap();
    let database = retention::owner(&nested).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::write(root.path().join("tracked.txt"), b"task edit\n").unwrap();
    git(root.path(), &["add", "tracked.txt"], None).unwrap();
    let index = fs::read(root.path().join(".git/index")).unwrap();
    fs::remove_dir_all(&nested).unwrap();
    fs::write(database.join("index.lock"), b"other Git operation\n").unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    let error = baseline.discard(root.path(), &paths).unwrap_err();
    assert!(error.to_string().contains("cannot lock Git index"), "{error}");
    assert_eq!(fs::read(root.path().join("tracked.txt")).unwrap(), b"task edit\n");
    assert_eq!(fs::read(root.path().join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(database.join("index.lock")).unwrap(), b"other Git operation\n");
    assert!(!nested.exists());
    fs::remove_file(database.join("index.lock")).unwrap();
    baseline.discard(root.path(), &paths).unwrap();
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[test]
fn lost_external_database_leaves_source_untouched_and_deleted_source_exportable() {
    let root = repository();
    let donor = repository();
    let nested = root.path().join("nested");
    git(
        donor.path(),
        &["worktree", "add", "--quiet", "--detach", nested.to_str().unwrap(), "HEAD"],
        None,
    )
    .unwrap();
    fs::write(nested.join("tracked.txt"), b"original linked source\n").unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::remove_dir_all(&nested).unwrap();
    donor.close().unwrap();
    fs::write(root.path().join("tracked.txt"), b"task edit\n").unwrap();
    let index = fs::read(root.path().join(".git/index")).unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    let patch = String::from_utf8(baseline.patch(root.path()).unwrap()).unwrap();
    assert!(patch.contains("-original linked source"));
    assert!(baseline.discard(root.path(), &paths).is_err());
    assert_eq!(fs::read(root.path().join("tracked.txt")).unwrap(), b"task edit\n");
    assert_eq!(fs::read(root.path().join(".git/index")).unwrap(), index);
    assert!(!nested.exists());
}

#[test]
fn deleted_parent_repository_restores_its_submodule_database_and_relative_link() {
    let root = repository();
    let donor = repository();
    let parent = root.path().join("parent");
    fs::rename(repository().keep(), &parent).unwrap();
    git(
        &parent,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "--quiet",
            donor.path().to_str().unwrap(),
            "nested",
        ],
        None,
    )
    .unwrap();
    let nested = parent.join("nested");
    let marker = fs::read(nested.join(".git")).unwrap();
    fs::write(nested.join("tracked.txt"), b"original staged submodule\n").unwrap();
    git(&nested, &["add", "tracked.txt"], None).unwrap();
    fs::write(nested.join("tracked.txt"), b"original working submodule\n").unwrap();
    let index = git(&nested, &["ls-files", "--stage", "-z"], None).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::remove_dir_all(&parent).unwrap();
    baseline.discard(root.path(), &baseline.changed_paths(root.path()).unwrap()).unwrap();
    assert_eq!(fs::read(nested.join(".git")).unwrap(), marker);
    assert_eq!(fs::read(nested.join("tracked.txt")).unwrap(), b"original working submodule\n");
    assert_eq!(git(&nested, &["ls-files", "--stage", "-z"], None).unwrap(), index);
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[test]
fn deleted_linked_worktree_restores_source_and_index_without_relocating_its_database() {
    let root = repository();
    let donor = repository();
    let nested = root.path().join("nested");
    git(
        donor.path(),
        &["worktree", "add", "--quiet", "--detach", nested.to_str().unwrap(), "HEAD"],
        None,
    )
    .unwrap();
    let git_file = fs::read(nested.join(".git")).unwrap();
    let git_directory =
        text(git(&nested, &["rev-parse", "--absolute-git-dir"], None).unwrap()).unwrap();
    fs::write(nested.join("tracked.txt"), b"user staged worktree\n").unwrap();
    git(&nested, &["add", "tracked.txt"], None).unwrap();
    fs::write(nested.join("tracked.txt"), b"user working worktree\n").unwrap();
    let index = git(&nested, &["ls-files", "--stage", "-z"], None).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::write(nested.join("tracked.txt"), b"task change\n").unwrap();
    fs::write(nested.join("task.txt"), b"task addition\n").unwrap();
    git(&nested, &["add", "."], None).unwrap();
    git(&nested, &["-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "task commit"], None)
        .unwrap();
    let task_commit = text(git(&nested, &["rev-parse", "HEAD"], None).unwrap()).unwrap();
    fs::remove_dir_all(&nested).unwrap();
    let recovered =
        baseline.discard(root.path(), &baseline.changed_paths(root.path()).unwrap()).unwrap();
    assert_eq!(fs::read(nested.join(".git")).unwrap(), git_file);
    assert_eq!(
        text(git(&nested, &["rev-parse", "--absolute-git-dir"], None).unwrap()).unwrap(),
        git_directory
    );
    assert_eq!(fs::read(nested.join("tracked.txt")).unwrap(), b"user working worktree\n");
    assert_eq!(git(&nested, &["ls-files", "--stage", "-z"], None).unwrap(), index);
    assert!(!nested.join("task.txt").exists());
    assert_eq!(recovered.len(), 1);
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(recovered[0].join("head.json")).unwrap()).unwrap();
    assert_eq!(record["repository"].as_str(), nested.to_str());
    assert_eq!(
        text(
            git(&nested, &["rev-parse", record["retained_commit_ref"].as_str().unwrap()], None)
                .unwrap()
        )
        .unwrap(),
        task_commit
    );
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[test]
fn deleted_submodule_restores_its_relative_git_link_and_original_index() {
    let root = repository();
    let donor = repository();
    git(
        root.path(),
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "--quiet",
            donor.path().to_str().unwrap(),
            "nested",
        ],
        None,
    )
    .unwrap();
    let nested = root.path().join("nested");
    let marker = fs::read(nested.join(".git")).unwrap();
    fs::write(nested.join("tracked.txt"), b"user staged submodule\n").unwrap();
    git(&nested, &["add", "tracked.txt"], None).unwrap();
    fs::write(nested.join("tracked.txt"), b"user working submodule\n").unwrap();
    let index = git(&nested, &["ls-files", "--stage", "-z"], None).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::write(nested.join("tracked.txt"), b"task edit\n").unwrap();
    git(&nested, &["add", "tracked.txt"], None).unwrap();
    fs::remove_dir_all(&nested).unwrap();
    baseline.discard(root.path(), &baseline.changed_paths(root.path()).unwrap()).unwrap();
    assert_eq!(fs::read(nested.join(".git")).unwrap(), marker);
    assert_eq!(fs::read(nested.join("tracked.txt")).unwrap(), b"user working submodule\n");
    assert_eq!(git(&nested, &["ls-files", "--stage", "-z"], None).unwrap(), index);
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}
