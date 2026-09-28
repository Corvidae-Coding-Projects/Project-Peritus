use super::*;

#[test]
fn deleted_nested_repository_restores_history_source_index_and_local_metadata() {
    let root = repository();
    let nested = root.path().join("nested");
    fs::rename(repository().keep(), &nested).unwrap();
    fs::write(nested.join("tracked.txt"), b"user staged\n").unwrap();
    git(&nested, &["add", "tracked.txt"], None).unwrap();
    fs::write(nested.join("tracked.txt"), b"user unstaged\n").unwrap();
    fs::write(nested.join("draft.txt"), b"user untracked\n").unwrap();
    fs::write(nested.join(".git/hooks/user-hook"), b"retained hook data\n").unwrap();
    git(&nested, &["config", "user.name", "Original Identity"], None).unwrap();
    fs::write(nested.join(".git/index.lock"), b"transient lock\n").unwrap();
    let head = git(&nested, &["rev-parse", "HEAD"], None).unwrap();
    let index = git(&nested, &["ls-files", "--stage", "-z"], None).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    let serialized = serde_json::to_vec(&baseline).unwrap();
    fs::remove_dir_all(&nested).unwrap();
    let baseline: ManagedBaseline = serde_json::from_slice(&serialized).unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    assert!(paths.contains(&PathBuf::from("nested")));
    baseline.discard(root.path(), &paths).expect("restore deleted original repository");
    assert_eq!(fs::read(nested.join("tracked.txt")).unwrap(), b"user unstaged\n");
    assert_eq!(fs::read(nested.join("draft.txt")).unwrap(), b"user untracked\n");
    assert_eq!(git(&nested, &["rev-parse", "HEAD"], None).unwrap(), head);
    assert_eq!(git(&nested, &["ls-files", "--stage", "-z"], None).unwrap(), index);
    assert_eq!(git(&nested, &["config", "user.name"], None).unwrap(), b"Original Identity\n");
    assert_eq!(fs::read(nested.join(".git/hooks/user-hook")).unwrap(), b"retained hook data\n");
    assert!(!nested.join(".git/index.lock").exists());
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[test]
fn deleting_an_original_unborn_repository_is_a_visible_candidate_change() {
    let root = repository();
    let nested = root.path().join("unborn");
    fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "--quiet"], None).unwrap();
    fs::write(nested.join("draft.txt"), b"original draft\n").unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::remove_dir_all(&nested).unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    assert_eq!(paths, vec![PathBuf::from("unborn")]);
    let patch = String::from_utf8(baseline.patch(root.path()).unwrap()).unwrap();
    assert!(patch.contains("-original draft"), "export must include deleted unborn source");
    baseline.discard(root.path(), &paths).unwrap();
    assert_eq!(fs::read(nested.join("draft.txt")).unwrap(), b"original draft\n");
    assert!(capture::nested_head(&nested).unwrap().is_none());
}

#[test]
fn retained_repositories_survive_gc_and_restore_recursive_history() {
    let root = repository();
    let nested = root.path().join("nested");
    fs::rename(repository().keep(), &nested).unwrap();
    let deep = nested.join("deep");
    fs::rename(repository().keep(), &deep).unwrap();
    fs::write(deep.join("tracked.txt"), b"deep user bytes\n").unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    let store = root.path().join(".git/peritus/retained-repositories/sha1");
    git(&store, &["gc", "--prune=now"], None).unwrap();
    fs::remove_dir_all(&nested).unwrap();
    baseline.discard(root.path(), &baseline.changed_paths(root.path()).unwrap()).unwrap();
    assert_eq!(fs::read(deep.join("tracked.txt")).unwrap(), b"deep user bytes\n");
    assert!(capture::nested_head(&deep).unwrap().is_some());
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[test]
fn corrupt_independent_backup_does_not_change_current_workspace_or_index() {
    let root = repository();
    let nested = root.path().join("nested");
    fs::rename(repository().keep(), &nested).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::remove_dir_all(&nested).unwrap();
    fs::write(root.path().join("tracked.txt"), b"task bytes\n").unwrap();
    git(root.path(), &["add", "tracked.txt"], None).unwrap();
    let index = fs::read(root.path().join(".git/index")).unwrap();
    let object = baseline.nested["nested"].retained.as_ref().unwrap();
    let object_path = root
        .path()
        .join(".git/peritus/retained-repositories/sha1/objects")
        .join(&object[..2])
        .join(&object[2..]);
    fs::remove_file(object_path).unwrap();
    assert!(baseline.discard(root.path(), &baseline.changed_paths(root.path()).unwrap()).is_err());
    assert_eq!(fs::read(root.path().join("tracked.txt")).unwrap(), b"task bytes\n");
    assert_eq!(fs::read(root.path().join(".git/index")).unwrap(), index);
    assert!(!nested.exists());
    assert!(!fs::read_dir(root.path()).unwrap().any(|entry| {
        entry.unwrap().file_name().to_string_lossy().starts_with(".peritus-repository-restore-")
    }));
}

#[test]
fn independently_retained_sha256_repository_restores_after_gc() {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "--quiet", "--object-format=sha256"], None).unwrap();
    let nested = root.path().join("nested");
    fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "--quiet", "--object-format=sha256"], None).unwrap();
    fs::write(nested.join("source"), b"sha256 bytes\n").unwrap();
    git(&nested, &["add", "source"], None).unwrap();
    git(
        &nested,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "original",
        ],
        None,
    )
    .unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    git(
        &root.path().join(".git/peritus/retained-repositories/sha256"),
        &["gc", "--prune=now"],
        None,
    )
    .unwrap();
    fs::remove_dir_all(&nested).unwrap();
    baseline.discard(root.path(), &baseline.changed_paths(root.path()).unwrap()).unwrap();
    assert_eq!(fs::read(nested.join("source")).unwrap(), b"sha256 bytes\n");
    assert_eq!(
        text(git(&nested, &["rev-parse", "--show-object-format"], None).unwrap()).unwrap(),
        "sha256"
    );
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[test]
fn independent_backup_does_not_depend_on_original_alternate_objects() {
    let root = repository();
    let donor = repository();
    let nested = root.path().join("nested");
    git(
        root.path(),
        &["clone", "--quiet", "--shared", donor.path().to_str().unwrap(), "nested"],
        None,
    )
    .unwrap();
    let head = git(&nested, &["rev-parse", "HEAD"], None).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    donor.close().unwrap();
    fs::remove_dir_all(&nested).unwrap();
    baseline.discard(root.path(), &baseline.changed_paths(root.path()).unwrap()).unwrap();
    assert_eq!(git(&nested, &["rev-parse", "HEAD"], None).unwrap(), head);
    git(&nested, &["fsck", "--no-reflogs"], None).unwrap();
    assert!(baseline.changed_paths(root.path()).unwrap().is_empty());
}

#[test]
fn shallow_repository_can_be_retained_and_recovered() {
    let root = repository();
    let donor = repository();
    fs::write(donor.path().join("tracked.txt"), b"second commit\n").unwrap();
    git(donor.path(), &["add", "tracked.txt"], None).unwrap();
    git(donor.path(), &["-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "second"], None)
        .unwrap();
    let nested = root.path().join("nested");
    let source = format!("file://{}", donor.path().display());
    git(root.path(), &["clone", "--quiet", "--depth=1", &source, "nested"], None).unwrap();
    let head = git(&nested, &["rev-parse", "HEAD"], None).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::remove_dir_all(&nested).unwrap();
    donor.close().unwrap();
    baseline.discard(root.path(), &baseline.changed_paths(root.path()).unwrap()).unwrap();
    assert_eq!(git(&nested, &["rev-parse", "HEAD"], None).unwrap(), head);
    assert_eq!(fs::read(nested.join("tracked.txt")).unwrap(), b"second commit\n");
    git(&nested, &["fsck", "--no-reflogs"], None).unwrap();
}

#[cfg(unix)]
#[test]
fn retained_metadata_preserves_permissions_and_symlinks_without_copying_their_targets() {
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    let root = repository();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("external-hook");
    fs::write(&target, b"outside hook data\n").unwrap();
    let nested = root.path().join("nested");
    fs::rename(repository().keep(), &nested).unwrap();
    fs::set_permissions(nested.join("tracked.txt"), fs::Permissions::from_mode(0o600)).unwrap();
    let hook = nested.join(".git/hooks/user-hook");
    fs::write(&hook, b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    symlink(&target, nested.join(".git/hooks/external")).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::remove_dir_all(&nested).unwrap();
    fs::write(&target, b"new outside data\n").unwrap();
    baseline.discard(root.path(), &baseline.changed_paths(root.path()).unwrap()).unwrap();
    assert_eq!(
        fs::metadata(nested.join("tracked.txt")).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::metadata(&hook).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(fs::read_link(nested.join(".git/hooks/external")).unwrap(), target);
    assert_eq!(fs::read(&target).unwrap(), b"new outside data\n");
}

#[test]
fn legacy_baseline_without_independent_backup_still_restores_existing_repository() {
    let root = repository();
    let nested = root.path().join("nested");
    fs::rename(repository().keep(), &nested).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    let mut encoded = serde_json::to_value(baseline).unwrap();
    encoded["nested"]["nested"].as_object_mut().unwrap().remove("retained");
    let baseline: ManagedBaseline = serde_json::from_value(encoded).unwrap();
    fs::write(nested.join("tracked.txt"), b"task changes\n").unwrap();
    baseline.discard(root.path(), &baseline.changed_paths(root.path()).unwrap()).unwrap();
    assert_eq!(fs::read(nested.join("tracked.txt")).unwrap(), b"committed\n");
    fs::remove_dir_all(&nested).unwrap();
    let error =
        baseline.discard(root.path(), &baseline.changed_paths(root.path()).unwrap()).unwrap_err();
    assert!(error.to_string().contains("no independent baseline backup"));
    assert!(!nested.exists());
}

#[test]
fn preparing_repository_recovery_does_not_publish_a_scratch_candidate() {
    let root = repository();
    let nested = root.path().join("nested");
    fs::rename(repository().keep(), &nested).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::remove_dir_all(&nested).unwrap();
    let paths = baseline.changed_paths(root.path()).unwrap();
    let before = ManagedBaseline::repository_fingerprint(root.path()).unwrap();
    let pending = retention::Replacements::prepare(&baseline, root.path(), &paths).unwrap();
    assert_eq!(
        ManagedBaseline::repository_fingerprint(root.path()).unwrap(),
        before,
        "a private restore stage must never appear in the user's candidate, even if interrupted"
    );
    drop(pending);
    assert!(!nested.exists());
}
