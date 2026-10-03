use super::*;

#[cfg(unix)]
#[test]
fn invalid_later_symlink_preimage_keeps_all_current_bytes_and_releases_prepared_locks() {
    let root = repository();
    let nested = root.path().join("nested");
    fs::rename(repository().keep(), &nested).unwrap();
    std::os::unix::fs::symlink("tracked.txt", nested.join("z-link")).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    let mut corrupt = baseline.clone();
    let object =
        text(git(&nested, &["hash-object", "-w", "--stdin"], Some(b"bad\0target")).unwrap())
            .unwrap();
    corrupt.nested.get_mut("nested").unwrap().entries.get_mut("z-link").unwrap().object = object;
    fs::write(root.path().join("tracked.txt"), b"root task bytes\n").unwrap();
    fs::write(nested.join("tracked.txt"), b"nested task bytes\n").unwrap();
    git(&nested, &["-c", "commit.gpgsign=false", "commit", "-qam", "task"], None).unwrap();
    fs::remove_file(nested.join("z-link")).unwrap();
    std::os::unix::fs::symlink("unrelated.txt", nested.join("z-link")).unwrap();
    let generated = root.path().join("generated");
    fs::rename(repository().keep(), &generated).unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    let head = git(&nested, &["rev-parse", "HEAD"], None).unwrap();
    let root_index = fs::read(root.path().join(".git/index")).unwrap();
    let nested_index = fs::read(nested.join(".git/index")).unwrap();
    assert!(corrupt.discard(root.path(), &paths).is_err());
    assert_eq!(fs::read(root.path().join("tracked.txt")).unwrap(), b"root task bytes\n");
    assert_eq!(fs::read(nested.join("tracked.txt")).unwrap(), b"nested task bytes\n");
    assert_eq!(fs::read_link(nested.join("z-link")).unwrap(), Path::new("unrelated.txt"));
    assert_eq!(git(&nested, &["rev-parse", "HEAD"], None).unwrap(), head);
    assert_eq!(fs::read(root.path().join(".git/index")).unwrap(), root_index);
    assert_eq!(fs::read(nested.join(".git/index")).unwrap(), nested_index);
    assert!(generated.join(".git").is_dir());
    for directory in [root.path(), nested.as_path()] {
        assert!(!directory.join(".git/index.lock").exists());
        assert!(!directory.join(".git/HEAD.lock").exists());
        assert!(!fs::read_dir(directory).unwrap().any(|entry| {
            entry.unwrap().file_name().to_string_lossy().starts_with(".peritus-restore-")
        }));
    }
    baseline.discard(root.path(), &paths).unwrap();
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn unwritable_later_restore_directory_keeps_source_index_and_new_repositories_intact() {
    use std::os::unix::fs::PermissionsExt as _;

    let root = repository();
    let later = root.path().join("z-later");
    fs::create_dir(&later).unwrap();
    fs::write(later.join("source.txt"), b"original later bytes\n").unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::write(root.path().join("tracked.txt"), b"retain earlier task bytes\n").unwrap();
    fs::write(later.join("source.txt"), b"retain later task bytes\n").unwrap();
    let generated = root.path().join("generated");
    fs::rename(repository().keep(), &generated).unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    let index = fs::read(root.path().join(".git/index")).unwrap();
    let permissions = fs::metadata(&later).unwrap().permissions();
    fs::set_permissions(&later, fs::Permissions::from_mode(0o555)).unwrap();
    let probe = tempfile::tempfile_in(&later);
    let result = baseline.discard(root.path(), &paths);
    // Restore permissions before asserting, including when the test runs as a privileged user.
    fs::set_permissions(&later, permissions).unwrap();
    if probe.is_ok() {
        assert!(result.is_ok(), "privileged restore failed: {result:?}");
        return;
    }
    assert!(result.is_err());
    assert_eq!(fs::read(root.path().join("tracked.txt")).unwrap(), b"retain earlier task bytes\n");
    assert_eq!(fs::read(later.join("source.txt")).unwrap(), b"retain later task bytes\n");
    assert_eq!(fs::read(root.path().join(".git/index")).unwrap(), index);
    assert!(generated.join(".git").is_dir());
    assert!(!root.path().join(".git/index.lock").exists());
    baseline.discard(root.path(), &paths).unwrap();
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[test]
fn later_head_lock_failure_aborts_earlier_prepared_git_transactions() {
    let root = repository();
    let first = root.path().join("a-first");
    let second = root.path().join("z-second");
    for nested in [&first, &second] {
        fs::rename(repository().keep(), nested).unwrap();
    }
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    for nested in [&first, &second] {
        fs::write(nested.join("tracked.txt"), b"task change\n").unwrap();
        git(nested, &["-c", "commit.gpgsign=false", "commit", "-qam", "task"], None).unwrap();
    }
    let first_head = git(&first, &["rev-parse", "HEAD"], None).unwrap();
    fs::write(second.join(".git/HEAD.lock"), b"existing lock\n").unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    assert!(baseline.discard(root.path(), &paths).is_err());
    assert_eq!(git(&first, &["rev-parse", "HEAD"], None).unwrap(), first_head);
    for nested in [&first, &second] {
        assert_eq!(fs::read(nested.join("tracked.txt")).unwrap(), b"task change\n");
        assert!(!nested.join(".git/index.lock").exists());
    }
    assert!(!first.join(".git/HEAD.lock").exists());
    assert_eq!(fs::read(second.join(".git/HEAD.lock")).unwrap(), b"existing lock\n");
    fs::remove_file(second.join(".git/HEAD.lock")).unwrap();
    baseline.discard(root.path(), &paths).unwrap();
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[test]
fn unborn_head_lock_is_prepared_before_source_changes_and_released_on_failure() {
    let root = repository();
    let nested = root.path().join("nested");
    fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "--quiet"], None).unwrap();
    fs::write(nested.join("source.txt"), b"original draft\n").unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::write(nested.join("source.txt"), b"task commit\n").unwrap();
    git(&nested, &["add", "."], None).unwrap();
    git(
        &nested,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "task",
        ],
        None,
    )
    .unwrap();
    fs::write(root.path().join("tracked.txt"), b"root task edit\n").unwrap();
    fs::write(nested.join(".git/HEAD.lock"), b"existing lock\n").unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    assert!(baseline.discard(root.path(), &paths).is_err());
    assert_eq!(fs::read(root.path().join("tracked.txt")).unwrap(), b"root task edit\n");
    assert_eq!(fs::read(nested.join("source.txt")).unwrap(), b"task commit\n");
    fs::remove_file(nested.join(".git/HEAD.lock")).unwrap();
    baseline.discard(root.path(), &paths).unwrap();
    assert!(capture::nested_head(&nested).unwrap().is_none());
    assert!(!nested.join(".git/HEAD.lock").exists());
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[test]
fn locked_nested_head_prevents_all_source_index_and_archive_effects_and_allows_retry() {
    let root = repository();
    let nested = root.path().join("nested");
    fs::rename(repository().keep(), &nested).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::write(nested.join("tracked.txt"), b"task commit\n").unwrap();
    git(&nested, &["-c", "commit.gpgsign=false", "commit", "-qam", "task"], None).unwrap();
    fs::write(root.path().join("tracked.txt"), b"root task change\n").unwrap();
    git(root.path(), &["add", "tracked.txt"], None).unwrap();
    let index = fs::read(root.path().join(".git/index")).unwrap();
    let generated = root.path().join("generated");
    fs::rename(repository().keep(), &generated).unwrap();
    let lock = nested.join(".git/HEAD.lock");
    fs::write(&lock, b"another Git operation").unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    assert!(baseline.discard(root.path(), &paths).is_err());
    assert_eq!(fs::read(root.path().join("tracked.txt")).unwrap(), b"root task change\n");
    assert_eq!(fs::read(nested.join("tracked.txt")).unwrap(), b"task commit\n");
    assert_eq!(fs::read(root.path().join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(&lock).unwrap(), b"another Git operation");
    assert!(generated.join(".git").is_dir());
    fs::remove_file(&lock).unwrap();
    baseline.discard(root.path(), &paths).unwrap();
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[test]
fn any_locked_repository_index_prevents_all_discard_effects_and_allows_retry() {
    for lock_nested in [false, true] {
        let root = repository();
        let nested = root.path().join("nested");
        fs::rename(repository().keep(), &nested).unwrap();
        let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
        fs::write(root.path().join("tracked.txt"), b"root task change\n").unwrap();
        fs::write(nested.join("tracked.txt"), b"nested task change\n").unwrap();
        let generated = root.path().join("generated");
        fs::rename(repository().keep(), &generated).unwrap();
        let paths = baseline.changed_paths(root.path()).unwrap();
        let owner = if lock_nested { nested.as_path() } else { root.path() };
        let lock = owner.join(".git/index.lock");
        fs::write(&lock, b"another Git operation").unwrap();
        assert!(baseline.discard(root.path(), &paths).is_err());
        assert_eq!(fs::read(root.path().join("tracked.txt")).unwrap(), b"root task change\n");
        assert_eq!(fs::read(nested.join("tracked.txt")).unwrap(), b"nested task change\n");
        assert_eq!(fs::read(&lock).unwrap(), b"another Git operation");
        assert!(generated.join(".git").is_dir());
        assert!(!root.path().join(".git/peritus/discarded").exists());
        fs::remove_file(&lock).unwrap();
        baseline.discard(root.path(), &paths).unwrap();
        assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
        assert!(!root.path().join(".git/index.lock").exists());
        assert!(!nested.join(".git/index.lock").exists());
    }
}

#[test]
fn invalid_current_index_does_not_restore_files_before_reporting_the_error() {
    let root = repository();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::write(root.path().join("tracked.txt"), b"retain task change\n").unwrap();
    fs::write(root.path().join(".git/index"), b"broken Git index").unwrap();
    assert!(baseline.discard(root.path(), &[PathBuf::from("tracked.txt")]).is_err());
    assert_eq!(fs::read(root.path().join("tracked.txt")).unwrap(), b"retain task change\n");
    assert!(!root.path().join(".git/index.lock").exists());
}

#[test]
fn prepared_restore_keeps_split_index_and_unrelated_staged_content_readable() {
    let root = repository();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::write(root.path().join("tracked.txt"), b"discard task change\n").unwrap();
    fs::write(root.path().join("unrelated.txt"), b"keep staged user change\n").unwrap();
    git(root.path(), &["add", "unrelated.txt"], None).unwrap();
    git(root.path(), &["update-index", "--split-index"], None).unwrap();
    baseline.discard(root.path(), &[PathBuf::from("tracked.txt")]).unwrap();
    assert_eq!(fs::read(root.path().join("tracked.txt")).unwrap(), b"committed\n");
    assert_eq!(
        git(root.path(), &["show", ":unrelated.txt"], None).unwrap(),
        b"keep staged user change\n"
    );
    assert_eq!(git(root.path(), &["show", ":tracked.txt"], None).unwrap(), b"committed\n");
}
