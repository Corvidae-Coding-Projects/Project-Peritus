//! Real Git ownership boundaries for nested repository candidates and restoration.
mod support;

use peritus_git::{
    CandidateRequest, CreateWorktree, ErrorKind, GitCancellation, GitRepository, RemovalPolicy,
    RepositoryOptions, RestoreRequest, SnapshotRequest, WorktreeAccess, WorktreeName,
};
use peritus_types::{SnapshotId, WorkspaceId};
use support::{RepositoryFixture, checked_git};

#[test]
fn tracked_child_content_rejects_registration_and_later_index_drift_before_parent_effects() {
    let fixture = RepositoryFixture::sha1();
    let repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").unwrap();
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("tracked_child").unwrap(),
            fixture.worktree_path("tracked_child"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .unwrap();
    let child = worktree.root().join("child");
    std::fs::create_dir(&child).unwrap();
    std::fs::write(child.join("owned"), b"parent bytes").unwrap();
    checked_git(worktree.root(), &["add", "child/owned"]);
    let parent_blob = checked_git(worktree.root(), &["rev-parse", ":child/owned"]);
    checked_git(&child, &["init", "--quiet"]);
    checked_git(&child, &["config", "user.name", "Child"]);
    checked_git(&child, &["config", "user.email", "child@example.invalid"]);
    checked_git(&child, &["add", "owned"]);
    checked_git(&child, &["commit", "-qm", "child baseline"]);
    std::fs::write(child.join("owned"), b"private child change").unwrap();
    let nested = GitRepository::open(RepositoryOptions::new(&child)).unwrap();
    let before = checked_git(worktree.root(), &["write-tree"]);
    assert_eq!(
        repository.register_nested_repository(&worktree, &nested).unwrap_err().kind(),
        ErrorKind::WorktreeConflict
    );
    assert_eq!(checked_git(worktree.root(), &["write-tree"]), before);
    assert_eq!(checked_git(worktree.root(), &["show", ":child/owned"]), "parent bytes");

    checked_git(worktree.root(), &["update-index", "--force-remove", "child/owned"]);
    let registrations = [repository.register_nested_repository(&worktree, &nested).unwrap()];
    let candidate = repository
        .create_candidate(
            CandidateRequest::new(&worktree, baseline.commit())
                .with_nested_repositories(&registrations),
        )
        .unwrap();
    let snapshot = repository
        .create_snapshot(SnapshotRequest::new(
            &worktree,
            &candidate,
            WorkspaceId::new([85; 16]).unwrap(),
            SnapshotId::new([86; 16]).unwrap(),
            baseline.commit(),
        ))
        .unwrap();
    checked_git(worktree.root(), &["update-index", "--force-remove", "child"]);
    checked_git(
        worktree.root(),
        &["update-index", "--add", "--cacheinfo", "100644", parent_blob.trim(), "child/owned"],
    );
    let before = checked_git(worktree.root(), &["write-tree"]);
    assert_eq!(
        repository
            .create_candidate(
                CandidateRequest::new(&worktree, baseline.commit())
                    .with_nested_repositories(&registrations)
            )
            .unwrap_err()
            .kind(),
        ErrorKind::WorktreeConflict
    );
    assert_eq!(
        repository
            .restore_snapshot(
                RestoreRequest::new(&worktree, &snapshot, baseline.commit())
                    .with_nested_repositories(&registrations)
            )
            .unwrap_err()
            .kind(),
        ErrorKind::WorktreeConflict
    );
    assert_eq!(checked_git(worktree.root(), &["write-tree"]), before);
    assert_eq!(std::fs::read(child.join("owned")).unwrap(), b"private child change");
}

#[test]
fn explicitly_registered_ignored_child_is_captured_as_its_exact_head_gitlink() {
    let fixture = RepositoryFixture::sha1();
    let repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").unwrap();
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("ignored_child").unwrap(),
            fixture.worktree_path("ignored_child"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .unwrap();
    std::fs::write(worktree.root().join(".gitignore"), b"child/\n").unwrap();
    checked_git(worktree.root(), &["clone", "--quiet", fixture.root.to_str().unwrap(), "child"]);
    let child = worktree.root().join("child");
    let nested = GitRepository::open(RepositoryOptions::new(&child)).unwrap();
    let registrations = [repository.register_nested_repository(&worktree, &nested).unwrap()];
    std::fs::write(child.join("tracked.txt"), b"private ignored dirty child").unwrap();
    let candidate = repository
        .create_candidate(
            CandidateRequest::new(&worktree, baseline.commit())
                .with_nested_repositories(&registrations),
        )
        .unwrap();
    let head = checked_git(&child, &["rev-parse", "HEAD"]);
    assert_eq!(
        checked_git(worktree.root(), &["ls-tree", &candidate.tree().to_string(), "child"]),
        format!("160000 commit {}\tchild", head.trim())
    );
    assert_eq!(std::fs::read(child.join("tracked.txt")).unwrap(), b"private ignored dirty child");
}

#[test]
fn registered_nested_gitlink_survives_parent_restore_and_cannot_be_removed_with_parent() {
    let fixture = RepositoryFixture::sha1();
    let repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("nested_run").expect("name"),
            fixture.worktree_path("nested_run"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("worktree");
    let child = worktree.root().join("child");
    checked_git(
        worktree.root(),
        &["clone", "--quiet", fixture.root.to_str().expect("path"), "child"],
    );
    let nested = GitRepository::open(RepositoryOptions::new(&child)).expect("nested repository");
    let registrations =
        [repository.register_nested_repository(&worktree, &nested).expect("register child")];
    assert_eq!(
        repository
            .create_candidate(CandidateRequest::new(&worktree, baseline.commit()))
            .expect_err("unregistered child")
            .kind(),
        ErrorKind::WorktreeConflict
    );
    std::fs::write(worktree.root().join(".gitignore"), b"ignored/\n").expect("ignore");
    std::fs::create_dir(worktree.root().join("ignored")).expect("ignored directory");
    std::fs::write(worktree.root().join("ignored/foreign"), b"preserve foreign ignored bytes")
        .expect("foreign data");
    let candidate = repository
        .create_candidate(
            CandidateRequest::new(&worktree, baseline.commit())
                .with_nested_repositories(&registrations),
        )
        .expect("registered candidate");
    let tree_entry =
        checked_git(worktree.root(), &["ls-tree", &candidate.tree().to_string(), "child"]);
    assert!(tree_entry.starts_with("160000 commit "));
    let snapshot = repository
        .create_snapshot(SnapshotRequest::new(
            &worktree,
            &candidate,
            WorkspaceId::new([81; 16]).expect("workspace"),
            SnapshotId::new([82; 16]).expect("snapshot"),
            baseline.commit(),
        ))
        .expect("snapshot");
    std::fs::write(worktree.root().join("tracked.txt"), b"parent change").expect("parent change");
    std::fs::write(worktree.root().join("temporary"), b"parent untracked").expect("untracked");
    std::fs::write(child.join("tracked.txt"), b"independently owned dirty child")
        .expect("child change");
    std::fs::write(worktree.root().join(".gitignore"), b"ignored/\nforeign/\n")
        .expect("changed ignore rules");
    std::fs::create_dir(worktree.root().join("foreign")).expect("new ignored directory");
    std::fs::write(worktree.root().join("foreign/preserve"), b"ignored before restore")
        .expect("new ignored data");
    checked_git(worktree.root(), &["config", "submodule.recurse", "true"]);
    repository
        .restore_snapshot(
            RestoreRequest::new(&worktree, &snapshot, baseline.commit())
                .with_nested_repositories(&registrations),
        )
        .expect("restore parent");
    assert_eq!(
        std::fs::read(child.join("tracked.txt")).expect("child data"),
        b"independently owned dirty child"
    );
    assert_eq!(
        std::fs::read(worktree.root().join("tracked.txt")).expect("parent data"),
        b"baseline\n"
    );
    assert_eq!(
        std::fs::read(worktree.root().join("ignored/foreign")).expect("foreign data"),
        b"preserve foreign ignored bytes"
    );
    assert!(!worktree.root().join("temporary").exists());
    assert_eq!(
        std::fs::read(worktree.root().join("foreign/preserve"))
            .expect("ignore rules changed but foreign data remains"),
        b"ignored before restore"
    );
    assert_eq!(
        repository
            .remove_worktree(&worktree, RemovalPolicy::ForceRegistered)
            .expect_err("parent cannot delete child")
            .kind(),
        ErrorKind::WorktreeConflict
    );
    assert!(child.join(".git").is_dir());
    // A token for the old metadata cannot authorize a replacement repository.
    std::fs::rename(child.join(".git"), child.join("old-git")).expect("move metadata");
    checked_git(&child, &["init", "--quiet", "--separate-git-dir", "new-git"]);
    assert!(
        repository
            .create_candidate(
                CandidateRequest::new(&worktree, baseline.commit())
                    .with_nested_repositories(&registrations)
            )
            .is_err()
    );
}

#[test]
fn parent_restore_rejects_nested_content_collision_before_mutation() {
    let fixture = RepositoryFixture::sha1();
    let repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("collision_run").expect("name"),
            fixture.worktree_path("collision_run"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("worktree");
    std::fs::create_dir(worktree.root().join("child")).expect("directory");
    std::fs::write(worktree.root().join("child/owned"), b"parent snapshot bytes")
        .expect("parent bytes");
    let candidate = repository
        .create_candidate(CandidateRequest::new(&worktree, baseline.commit()))
        .expect("candidate");
    let snapshot = repository
        .create_snapshot(SnapshotRequest::new(
            &worktree,
            &candidate,
            WorkspaceId::new([83; 16]).expect("workspace"),
            SnapshotId::new([84; 16]).expect("snapshot"),
            baseline.commit(),
        ))
        .expect("snapshot");
    checked_git(worktree.root(), &["rm", "-r", "--cached", "--", "child"]);
    std::fs::remove_dir_all(worktree.root().join("child")).expect("remove fixture owned child");
    checked_git(
        worktree.root(),
        &["clone", "--quiet", fixture.root.to_str().expect("path"), "child"],
    );
    let child =
        GitRepository::open(RepositoryOptions::new(worktree.root().join("child"))).expect("child");
    let registrations =
        [repository.register_nested_repository(&worktree, &child).expect("register")];
    let index_before = checked_git(worktree.root(), &["write-tree"]);
    assert_eq!(
        repository
            .restore_snapshot(
                RestoreRequest::new(&worktree, &snapshot, baseline.commit())
                    .with_nested_repositories(&registrations)
            )
            .expect_err("collision")
            .kind(),
        ErrorKind::WorktreeConflict
    );
    assert_eq!(checked_git(worktree.root(), &["write-tree"]), index_before);
    assert!(!worktree.root().join("child/owned").exists());
}

#[test]
fn ignored_directory_with_tracked_content_is_inspected_and_cancellation_preserves_index() {
    let fixture = RepositoryFixture::sha1();
    let mut repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("ignored_run").expect("name"),
            fixture.worktree_path("ignored_run"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("worktree");
    std::fs::create_dir(worktree.root().join("owned")).expect("directory");
    std::fs::write(worktree.root().join("owned/file"), b"tracked").expect("tracked file");
    checked_git(worktree.root(), &["add", "owned/file"]);
    std::fs::write(worktree.root().join(".gitignore"), b"owned/\n").expect("ignore");
    std::fs::create_dir(worktree.root().join("owned/.git")).expect("foreign metadata");
    assert_eq!(
        repository
            .create_candidate(CandidateRequest::new(&worktree, baseline.commit()))
            .expect_err("tracked ownership overrides ignore")
            .kind(),
        ErrorKind::WorktreeConflict
    );
    let before = checked_git(worktree.root(), &["write-tree"]);
    let cancellation = GitCancellation::new();
    repository.set_cancellation(cancellation.clone());
    cancellation.cancel();
    assert_eq!(
        repository
            .create_candidate(CandidateRequest::new(&worktree, baseline.commit()))
            .expect_err("cancel before mutation")
            .kind(),
        ErrorKind::Cancelled
    );
    assert_eq!(checked_git(worktree.root(), &["write-tree"]), before);
}
