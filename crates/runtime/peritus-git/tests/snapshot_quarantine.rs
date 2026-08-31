//! Real Git coverage for containing divergent retained snapshot references.

mod support;

use peritus_git::{
    CandidateRequest, CreateWorktree, ErrorKind, Operation, RemovalPolicy, SnapshotRequest,
    WorktreeAccess, WorktreeName,
};
use peritus_types::{SnapshotId, WorkspaceId};

use support::{RepositoryFixture, checked_git, git};

#[test]
fn divergent_snapshot_reference_is_atomically_quarantined() {
    let fixture = RepositoryFixture::sha1();
    let repository = fixture.open();
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("snapshot_quarantine").expect("name"),
            fixture.worktree_path("snapshot_quarantine"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("worktree");
    std::fs::write(worktree.root().join("tracked.txt"), b"candidate\n").expect("candidate");
    let candidate = repository
        .create_candidate(CandidateRequest::new(&worktree, baseline.commit()))
        .expect("candidate tree");
    let snapshot = repository
        .create_snapshot(SnapshotRequest::new(
            &worktree,
            &candidate,
            WorkspaceId::new([7; 16]).expect("workspace ID"),
            SnapshotId::new([8; 16]).expect("snapshot ID"),
            baseline.commit(),
        ))
        .expect("snapshot");
    let healthy = repository
        .quarantine_snapshot(snapshot.manifest())
        .expect_err("healthy snapshot cannot be quarantined");
    assert_eq!(healthy.kind(), ErrorKind::InvalidInput);
    assert_eq!(healthy.operation(), Operation::QuarantineSnapshot);

    checked_git(
        &fixture.root,
        &[
            "update-ref",
            snapshot.reference().as_str(),
            &baseline.commit().to_string(),
            &snapshot.commit().to_string(),
        ],
    );
    let divergent = repository
        .reopen_snapshot(snapshot.manifest())
        .expect_err("divergent reference cannot reopen");
    assert_eq!(divergent.kind(), ErrorKind::SnapshotConflict);

    let contained = repository
        .quarantine_snapshot(snapshot.manifest())
        .expect("quarantine divergent reference");
    assert_eq!(contained.active_reference(), snapshot.reference());
    assert_eq!(contained.observed_commit(), baseline.commit());
    assert_eq!(
        git(&fixture.root, &["show-ref", "--verify", "--quiet", snapshot.reference().as_str()])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        checked_git(
            &fixture.root,
            &["rev-parse", "--verify", contained.quarantine_reference().as_str()]
        ),
        baseline.commit().to_string()
    );
    assert_eq!(
        repository.quarantine_snapshot(snapshot.manifest()).expect("repeat quarantine"),
        contained
    );

    repository
        .remove_worktree(&worktree, RemovalPolicy::ForceRegistered)
        .expect("cleanup worktree");
}
