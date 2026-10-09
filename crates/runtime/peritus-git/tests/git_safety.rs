//! Real Git regressions for effect safety and detached topology.

mod support;

use peritus_git::{
    CandidateRequest, CreateWorktree, ReconcileDisposition, ReconcileExpectation, RemovalPolicy,
    RestoreRequest, SnapshotRequest, StatusKind, WorktreeAccess, WorktreeName,
};
use peritus_types::{SnapshotId, WorkspaceId};

use support::{RepositoryFixture, checked_git};

#[test]
fn configured_external_filter_is_rejected_before_execution_or_staging() {
    let fixture = RepositoryFixture::sha1();
    let repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("filter_run").expect("name"),
            fixture.worktree_path("filter_run"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("worktree");
    let marker = fixture.temporary.path().join("filter-marker");
    let driver = format!("sh -c 'touch {}; cat'", marker.display());
    checked_git(&fixture.root, &["config", "filter.qa.clean", &driver]);
    std::fs::write(worktree.root().join(".gitattributes"), b"*.txt filter=qa\n")
        .expect("attributes");
    std::fs::write(worktree.root().join("tracked.txt"), b"would invoke filter\n")
        .expect("dirty file");
    let index_before = checked_git(worktree.root(), &["rev-parse", ":tracked.txt"]);

    let error = repository
        .create_candidate(CandidateRequest::new(&worktree, baseline.commit()))
        .expect_err("configured filter must be unsupported");
    assert_eq!(error.kind(), peritus_git::ErrorKind::UnsupportedRepository);
    assert!(!marker.exists(), "external filter executed despite fail-closed check");
    assert_eq!(checked_git(worktree.root(), &["rev-parse", ":tracked.txt"]), index_before);

    checked_git(&fixture.root, &["config", "--unset", "filter.qa.clean"]);
    repository
        .remove_worktree(&worktree, RemovalPolicy::ForceRegistered)
        .expect("cleanup worktree");
}

#[test]
fn status_overrides_local_submodule_ignore_configuration() {
    let fixture = RepositoryFixture::sha1();
    let child = fixture.temporary.path().join("submodule-source");
    checked_git(fixture.temporary.path(), &["init", "--quiet", child.to_str().expect("path")]);
    checked_git(&child, &["config", "user.name", "Peritus Test"]);
    checked_git(&child, &["config", "user.email", "peritus@example.invalid"]);
    std::fs::write(child.join("child.txt"), b"baseline\n").expect("child file");
    checked_git(&child, &["add", "--", "child.txt"]);
    checked_git(&child, &["commit", "--quiet", "-m", "child baseline"]);
    checked_git(
        &fixture.root,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "--quiet",
            child.to_str().expect("path"),
            "child",
        ],
    );
    checked_git(&fixture.root, &["commit", "--quiet", "-am", "add submodule"]);

    let repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("submodule_run").expect("name"),
            fixture.worktree_path("submodule_run"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("worktree");
    checked_git(
        worktree.root(),
        &["-c", "protocol.file.allow=always", "submodule", "update", "--init", "--quiet"],
    );
    checked_git(&fixture.root, &["config", "submodule.child.ignore", "all"]);
    std::fs::write(worktree.root().join("child/child.txt"), b"dirty\n").expect("dirty child");

    let status = repository.status(&worktree).expect("status");
    assert!(status.entries().iter().any(|entry| {
        entry.path() == "child"
            && matches!(
                entry.kind(),
                StatusKind::Ordinary { submodule, .. } if submodule.modified_content()
            )
    }));
    repository
        .remove_worktree(&worktree, RemovalPolicy::ForceRegistered)
        .expect("cleanup worktree");
}

#[test]
fn attached_head_at_same_commit_is_not_accepted_as_managed_topology() {
    let fixture = RepositoryFixture::sha1();
    let repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("topology_run").expect("name"),
            fixture.worktree_path("topology_run"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("worktree");
    let candidate = repository
        .create_candidate(CandidateRequest::new(&worktree, baseline.commit()))
        .expect("candidate");
    let snapshot = repository
        .create_snapshot(SnapshotRequest::new(
            &worktree,
            &candidate,
            WorkspaceId::new([3; 16]).expect("workspace ID"),
            SnapshotId::new([4; 16]).expect("snapshot ID"),
            baseline.commit(),
        ))
        .expect("snapshot");
    checked_git(worktree.root(), &["switch", "--quiet", "-c", "attached-same-head"]);

    let status = repository.status(&worktree).expect("attached status");
    assert!(!status.is_detached());
    assert_eq!(status.head(), baseline.commit());
    let reconciled = repository
        .reconcile(ReconcileExpectation::new(&worktree, baseline.commit(), baseline.tree()))
        .expect("reconcile attached topology");
    assert_eq!(
        reconciled.disposition(),
        &ReconcileDisposition::Indeterminate(vec![peritus_git::DirtyReason::AttachedHead])
    );
    assert!(
        repository.create_candidate(CandidateRequest::new(&worktree, baseline.commit())).is_err()
    );
    assert!(
        repository
            .restore_snapshot(RestoreRequest::new(&worktree, &snapshot, baseline.commit()))
            .is_err()
    );

    repository.release_snapshot(&snapshot).expect("release snapshot");
    repository
        .remove_worktree(&worktree, RemovalPolicy::ForceRegistered)
        .expect("cleanup worktree");
}

#[test]
fn recovers_existing_exact_worktree_after_interrupted_registration() {
    let fixture = RepositoryFixture::sha1();
    let repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let destination = fixture.worktree_path("interrupted_run");
    let commit = baseline.commit().to_string();
    checked_git(
        &fixture.root,
        &[
            "-c",
            "core.autocrlf=false",
            "worktree",
            "add",
            "--quiet",
            "--detach",
            destination.to_str().expect("path"),
            &commit,
        ],
    );
    let recovered = repository
        .recover_existing_worktree(CreateWorktree::new(
            WorktreeName::new("interrupted_run").expect("name"),
            &destination,
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("recover existing worktree");
    assert!(repository.inspect_worktree(&recovered).expect("inspect").is_detached());
    repository.remove_worktree(&recovered, RemovalPolicy::RequireClean).expect("cleanup worktree");
}
