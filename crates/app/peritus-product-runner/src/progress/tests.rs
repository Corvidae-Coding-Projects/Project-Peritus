use std::{fs, process::Command};

use super::*;

#[cfg(unix)]
#[test]
fn scoped_checkpoint_does_not_read_through_a_replaced_parent_symlink() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("file.txt"), b"outside one\n").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("directory")).unwrap();
    let paths = vec![PathBuf::from("directory"), PathBuf::from("directory/file.txt")];
    let before = WorkspaceCheckpoint::scoped(root.path(), paths.clone()).unwrap().digest();
    fs::write(outside.path().join("file.txt"), b"outside two\n").unwrap();
    assert_eq!(before, WorkspaceCheckpoint::scoped(root.path(), paths).unwrap().digest());
}

#[cfg(unix)]
#[test]
fn checkpoint_tracks_nonexecutable_permissions_on_committed_files() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = tempfile::tempdir().unwrap();
    run(root.path(), &["init", "--quiet"]);
    fs::write(root.path().join("secret.txt"), b"private\n").unwrap();
    fs::set_permissions(root.path().join("secret.txt"), fs::Permissions::from_mode(0o600)).unwrap();
    run(root.path(), &["add", "."]);
    run(
        root.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "baseline",
        ],
    );
    let before = WorkspaceCheckpoint::capture(root.path()).unwrap().digest();
    fs::set_permissions(root.path().join("secret.txt"), fs::Permissions::from_mode(0o644)).unwrap();
    assert_ne!(before, WorkspaceCheckpoint::capture(root.path()).unwrap().digest());
}

#[test]
fn checkpoint_accepts_a_directory_replaced_by_a_file() {
    let root = tempfile::tempdir().unwrap();
    run(root.path(), &["init", "--quiet"]);
    fs::create_dir(root.path().join("directory")).unwrap();
    fs::write(root.path().join("directory/file.txt"), b"original\n").unwrap();
    run(root.path(), &["add", "."]);
    run(
        root.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "baseline",
        ],
    );
    let before = WorkspaceCheckpoint::capture(root.path()).unwrap().digest();
    fs::remove_file(root.path().join("directory/file.txt")).unwrap();
    fs::remove_dir(root.path().join("directory")).unwrap();
    fs::write(root.path().join("directory"), b"replacement\n").unwrap();
    assert_ne!(
        before,
        WorkspaceCheckpoint::capture(root.path()).expect("valid structural edit").digest()
    );
}

#[test]
fn checkpoint_changes_only_when_candidate_content_changes() {
    let root = tempfile::tempdir().expect("root");
    run(root.path(), &["init", "--quiet"]);
    run(root.path(), &["config", "user.email", "peritus@example.invalid"]);
    run(root.path(), &["config", "user.name", "Peritus Test"]);
    fs::write(root.path().join("tracked.txt"), "baseline").expect("write baseline");
    run(root.path(), &["add", "."]);
    run(root.path(), &["commit", "--quiet", "-m", "fixture"]);

    let clean = WorkspaceCheckpoint::capture(root.path()).expect("clean");
    let same = WorkspaceCheckpoint::capture(root.path()).expect("same");
    assert_eq!(clean, same);

    fs::write(root.path().join("tracked.txt"), "changed").expect("write change");
    let changed = WorkspaceCheckpoint::capture(root.path()).expect("changed");
    assert_ne!(clean, changed);

    fs::write(root.path().join("new.txt"), "untracked").expect("write untracked");
    let untracked = WorkspaceCheckpoint::capture(root.path()).expect("untracked");
    assert_ne!(changed, untracked);
}

#[test]
fn checkpoint_changes_when_the_task_creates_a_commit() {
    let root = tempfile::tempdir().expect("root");
    run(root.path(), &["init", "--quiet"]);
    run(root.path(), &["config", "user.email", "peritus@example.invalid"]);
    run(root.path(), &["config", "user.name", "Peritus Test"]);
    run(root.path(), &["commit", "--quiet", "--allow-empty", "-m", "fixture"]);
    let before = WorkspaceCheckpoint::capture(root.path()).expect("before commit");
    let source_before = crate::ProductRunner::candidate_source_digest(root.path()).unwrap();

    run(root.path(), &["commit", "--quiet", "--allow-empty", "-m", "task effect"]);
    let after = WorkspaceCheckpoint::capture(root.path()).expect("after commit");

    assert_ne!(before, after);
    assert_eq!(source_before, crate::ProductRunner::candidate_source_digest(root.path()).unwrap());
}

#[test]
fn checkpoint_tracks_committed_nested_source_and_unborn_source() {
    let root = tempfile::tempdir().unwrap();
    run(root.path(), &["init", "--quiet"]);
    run(
        root.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--allow-empty",
            "-qm",
            "root",
        ],
    );
    let child = root.path().join("application");
    fs::create_dir(&child).unwrap();
    run(&child, &["init", "--quiet"]);
    fs::write(child.join("source.txt"), b"original\n").unwrap();
    let unborn = WorkspaceCheckpoint::capture(root.path()).unwrap().digest();
    fs::write(child.join("source.txt"), b"changed\n").unwrap();
    assert_ne!(unborn, WorkspaceCheckpoint::capture(root.path()).unwrap().digest());
    run(&child, &["add", "."]);
    run(
        &child,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "first",
        ],
    );
    let committed = WorkspaceCheckpoint::capture(root.path()).unwrap().digest();
    fs::write(child.join("source.txt"), b"committed replacement\n").unwrap();
    run(
        &child,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-am",
            "second",
        ],
    );
    assert_ne!(committed, WorkspaceCheckpoint::capture(root.path()).unwrap().digest());
}

#[cfg(unix)]
#[test]
fn checkpoint_tracks_symlink_target_without_following_it() {
    let root = tempfile::tempdir().unwrap();
    run(root.path(), &["init", "--quiet"]);
    run(
        root.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--allow-empty",
            "-qm",
            "root",
        ],
    );
    let link = root.path().join("link");
    std::os::unix::fs::symlink("missing-one", &link).unwrap();
    let before = WorkspaceCheckpoint::capture(root.path()).unwrap().digest();
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("missing-two", &link).unwrap();
    assert_ne!(before, WorkspaceCheckpoint::capture(root.path()).unwrap().digest());
}

#[cfg(unix)]
#[test]
fn checkpoint_changes_when_candidate_permissions_change() {
    use std::os::unix::fs::PermissionsExt as _;

    let root = tempfile::tempdir().expect("root");
    run(root.path(), &["init", "--quiet"]);
    run(root.path(), &["config", "user.email", "peritus@example.invalid"]);
    run(root.path(), &["config", "user.name", "Peritus Test"]);
    fs::write(root.path().join("baseline.txt"), "baseline").expect("write baseline");
    run(root.path(), &["add", "."]);
    run(root.path(), &["commit", "--quiet", "-m", "fixture"]);
    let candidate = root.path().join("private.key");
    fs::write(&candidate, "secret").expect("write candidate");
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o644))
        .expect("initial permissions");
    let before = WorkspaceCheckpoint::capture(root.path()).expect("before permissions");

    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o600)).expect("fixed permissions");
    let after = WorkspaceCheckpoint::capture(root.path()).expect("after permissions");

    assert_ne!(before, after);
}

fn run(root: &Path, arguments: &[&str]) {
    assert!(Command::new("git").args(arguments).current_dir(root).status().expect("git").success());
}
