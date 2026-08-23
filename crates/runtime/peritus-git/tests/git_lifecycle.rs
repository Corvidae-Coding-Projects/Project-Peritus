//! Real Git subprocess coverage for the public repository lifecycle.

mod support;

use peritus_git::{
    CandidateRequest, CandidateSnapshotManifest, CandidateTreeManifest, CreateWorktree,
    ObjectFormat, ReconcileDisposition, ReconcileExpectation, RemovalPolicy, RestoreRequest,
    SnapshotRequest, StatusKind, WorktreeAccess, WorktreeName, WorktreeRegistrationManifest,
};
use peritus_types::{SnapshotId, WorkspaceId};

use support::{RepositoryFixture, checked_git, git};

#[test]
fn opens_resolves_and_manages_an_exact_detached_worktree() {
    let fixture = RepositoryFixture::sha1();
    let repository = fixture.open();
    assert_eq!(repository.identity().object_format(), ObjectFormat::Sha1);
    assert_eq!(repository.identity().repository_root(), fixture.root);
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let destination = fixture.worktree_path("writer");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("writer").expect("name"),
            &destination,
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("create worktree");
    let observed = repository.inspect_worktree(&worktree).expect("inspect worktree");
    assert!(observed.is_detached());
    assert_eq!(observed.head(), baseline.commit());
    assert_eq!(worktree.root(), std::fs::canonicalize(&destination).expect("canonical path"));
    assert!(repository.status(&worktree).expect("clean status").is_clean());

    std::fs::write(worktree.root().join("tracked.txt"), b"dirty\n").expect("dirty file");
    let error = repository
        .remove_worktree(&worktree, RemovalPolicy::RequireClean)
        .expect_err("dirty removal must fail");
    assert_eq!(error.kind(), peritus_git::ErrorKind::DirtyWorktree);
    repository
        .remove_worktree(&worktree, RemovalPolicy::ForceRegistered)
        .expect("force exact registration");
    assert!(!destination.exists());
    assert_eq!(checked_git(&fixture.root, &["rev-parse", "HEAD"]), baseline.commit().to_string());
}

#[test]
fn parses_tracked_untracked_ignored_and_renamed_status() {
    let fixture = RepositoryFixture::sha1();
    let repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("status_run").expect("name"),
            fixture.worktree_path("status_run"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("worktree");
    std::fs::rename(worktree.root().join("tracked.txt"), worktree.root().join("renamed.txt"))
        .expect("rename tracked file");
    std::fs::write(worktree.root().join("untracked.txt"), b"new\n").expect("untracked file");
    std::fs::write(worktree.root().join(".gitignore"), b"ignored.log\n").expect("ignore file");
    std::fs::write(worktree.root().join("ignored.log"), b"ignored\n").expect("ignored file");
    checked_git(worktree.root(), &["add", "--", ".gitignore", "renamed.txt", "tracked.txt"]);
    let status = repository.status(&worktree).expect("status");
    assert!(status.entries().iter().any(|entry| {
        entry.path() == "renamed.txt" && matches!(entry.kind(), StatusKind::Renamed { .. })
    }));
    assert!(status.entries().iter().any(|entry| {
        entry.path() == "untracked.txt" && matches!(entry.kind(), StatusKind::Untracked)
    }));
    assert!(status.entries().iter().any(|entry| {
        entry.path() == "ignored.log" && matches!(entry.kind(), StatusKind::Ignored)
    }));
    repository
        .remove_worktree(&worktree, RemovalPolicy::ForceRegistered)
        .expect("cleanup worktree");
}

#[test]
fn parses_real_unmerged_index_as_indeterminate_reconciliation() {
    let fixture = RepositoryFixture::sha1();
    let primary_branch = checked_git(&fixture.root, &["symbolic-ref", "--short", "HEAD"]);
    checked_git(&fixture.root, &["checkout", "--quiet", "-b", "conflicting-side"]);
    std::fs::write(fixture.root.join("tracked.txt"), b"side\n").expect("side content");
    checked_git(&fixture.root, &["commit", "--quiet", "-am", "side"]);
    checked_git(&fixture.root, &["checkout", "--quiet", &primary_branch]);
    std::fs::write(fixture.root.join("tracked.txt"), b"primary\n").expect("primary content");
    checked_git(&fixture.root, &["commit", "--quiet", "-am", "primary"]);

    let repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("conflict_run").expect("name"),
            fixture.worktree_path("conflict_run"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("worktree");
    let merge = git(worktree.root(), &["merge", "--no-edit", "conflicting-side"]);
    assert!(!merge.status.success(), "merge unexpectedly avoided conflict");
    let status = repository.status(&worktree).expect("conflicted porcelain status");
    assert!(status.index_tree().is_none());
    assert!(status.entries().iter().any(|entry| {
        entry.path() == "tracked.txt" && matches!(entry.kind(), StatusKind::Unmerged { .. })
    }));
    let reconciled = repository
        .reconcile(ReconcileExpectation::new(&worktree, baseline.commit(), baseline.tree()))
        .expect("classify conflict");
    assert_eq!(
        reconciled.disposition(),
        &ReconcileDisposition::Indeterminate(vec![peritus_git::DirtyReason::Conflict])
    );
    repository
        .remove_worktree(&worktree, RemovalPolicy::ForceRegistered)
        .expect("remove conflicted worktree");
}

#[test]
fn candidate_snapshot_restart_and_restore_preserve_head_and_history() {
    let fixture = RepositoryFixture::sha1();
    let repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("snapshot_run").expect("name"),
            fixture.worktree_path("snapshot_run"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("worktree");
    std::fs::write(worktree.root().join("tracked.txt"), b"candidate one\n").expect("candidate");
    std::fs::write(worktree.root().join("extra.txt"), b"retained\n").expect("extra");
    let candidate = repository
        .create_candidate(CandidateRequest::new(&worktree, baseline.commit()))
        .expect("candidate tree");
    assert_ne!(candidate.tree(), baseline.tree());
    assert_eq!(candidate.status().index_tree(), Some(candidate.tree()));
    let workspace_id = WorkspaceId::new([1; 16]).expect("workspace ID");
    let snapshot_id = SnapshotId::new([2; 16]).expect("snapshot ID");
    let snapshot = repository
        .create_snapshot(SnapshotRequest::new(
            &worktree,
            &candidate,
            workspace_id,
            snapshot_id,
            baseline.commit(),
        ))
        .expect("snapshot");
    assert_eq!(snapshot.tree(), candidate.tree());
    assert_eq!(repository.inspect_worktree(&worktree).expect("head").head(), baseline.commit());

    std::fs::write(worktree.root().join("tracked.txt"), b"candidate two\n").expect("second");
    std::fs::write(worktree.root().join("transient.txt"), b"remove me\n").expect("transient");
    let second = repository
        .create_candidate(CandidateRequest::new(&worktree, baseline.commit()))
        .expect("second candidate");
    assert_ne!(second.tree(), snapshot.tree());

    let registration_bytes =
        worktree.registration_manifest().expect("registration manifest").bytes().to_vec();
    let candidate_bytes = candidate.manifest().bytes().to_vec();
    let snapshot_bytes = snapshot.manifest().bytes().to_vec();
    let mut trailing_snapshot = snapshot_bytes.clone();
    trailing_snapshot.push(0);
    assert!(CandidateSnapshotManifest::decode(&trailing_snapshot).is_err());
    let mut unknown_registration = registration_bytes.clone();
    let magic_length =
        u32::from_be_bytes(unknown_registration[..4].try_into().expect("manifest string length"))
            as usize;
    unknown_registration[4 + magic_length..6 + magic_length].copy_from_slice(&2_u16.to_be_bytes());
    assert!(WorktreeRegistrationManifest::decode(&unknown_registration).is_err());
    drop(second);
    drop(snapshot);
    drop(candidate);
    drop(worktree);
    drop(repository);
    let reopened = fixture.open();
    let registration_manifest =
        WorktreeRegistrationManifest::decode(&registration_bytes).expect("decode registration");
    CandidateTreeManifest::decode(&candidate_bytes).expect("decode candidate manifest");
    let snapshot_manifest =
        CandidateSnapshotManifest::decode(&snapshot_bytes).expect("decode snapshot manifest");
    let worktree =
        reopened.reopen_worktree(&registration_manifest).expect("reopen worktree registration");
    let snapshot = reopened.reopen_snapshot(&snapshot_manifest).expect("reopen snapshot");
    let restored = reopened
        .restore_snapshot(RestoreRequest::new(&worktree, &snapshot, baseline.commit()))
        .expect("restore retained snapshot after reopen");
    assert_eq!(restored.restored_tree(), snapshot.tree());
    assert_eq!(
        std::fs::read(worktree.root().join("tracked.txt")).expect("read"),
        b"candidate one\n"
    );
    assert!(!worktree.root().join("transient.txt").exists());
    let reconciled = reopened
        .reconcile(ReconcileExpectation::new(&worktree, baseline.commit(), snapshot.tree()))
        .expect("reconcile restored tree");
    assert_eq!(reconciled.disposition(), &ReconcileDisposition::Clean);

    let review = reopened
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("review").expect("name"),
            fixture.worktree_path("review"),
            snapshot.baseline(),
            WorktreeAccess::ReadOnly,
        ))
        .expect("read-only snapshot worktree");
    assert!(reopened.status(&review).expect("review status").is_clean());
    assert!(reopened.create_candidate(CandidateRequest::new(&review, snapshot.commit())).is_err());
    reopened.remove_worktree(&review, RemovalPolicy::RequireClean).expect("remove review worktree");
    reopened.release_snapshot(&snapshot).expect("release snapshot ref");
    reopened.remove_worktree(&worktree, RemovalPolicy::ForceRegistered).expect("remove writer");
    assert_eq!(checked_git(&fixture.root, &["rev-parse", "HEAD"]), baseline.commit().to_string());
}

#[test]
fn opens_sha256_repository_when_supported_by_git() {
    let Some(fixture) = RepositoryFixture::new(Some("sha256")) else {
        return;
    };
    let repository = fixture.open();
    assert_eq!(repository.identity().object_format(), ObjectFormat::Sha256);
    assert_eq!(
        repository.resolve_baseline("HEAD").expect("SHA-256 baseline").commit().to_string().len(),
        64
    );
}
