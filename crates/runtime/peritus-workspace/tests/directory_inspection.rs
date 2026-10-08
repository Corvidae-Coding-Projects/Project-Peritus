//! Child-local inspection diagnostics preserve siblings and exact raw names.

use peritus_git::{CreateWorktree, GitRepository, RepositoryOptions, WorktreeAccess, WorktreeName};
use peritus_test_support::{FixturePath, TemporaryRepositoryBuilder};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};
use peritus_workspace::{
    DirectoryDiagnosticKind, ReadOnlyOpenRequest, ReadOnlyWorkspace, SnapshotIdentity,
};
use tempfile::TempDir;

#[cfg(unix)]
#[test]
fn directory_listing_keeps_supported_children_and_reports_unrepresentable_ones() {
    use std::os::unix::{ffi::OsStrExt as _, fs::symlink};

    let temp = TempDir::new().expect("temporary root");
    let mut source =
        TemporaryRepositoryBuilder::new(temp.path().join("peritus-test-directory-source"))
            .build()
            .expect("source repository");
    source
        .write_text(&FixturePath::new("tracked.txt").expect("fixture path"), "baseline\n")
        .expect("baseline file");
    let baseline_text = source.commit_all("baseline").expect("baseline commit");
    let repository =
        GitRepository::open(RepositoryOptions::new(source.root())).expect("repository");
    let baseline = repository.resolve_baseline(baseline_text.as_str()).expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("reader").expect("worktree name"),
            temp.path().join("reader"),
            baseline,
            WorktreeAccess::ReadOnly,
        ))
        .expect("read-only worktree");
    let workspace = ReadOnlyWorkspace::open(ReadOnlyOpenRequest::new(
        repository,
        worktree,
        SnapshotIdentity::new(
            WorkspaceId::new([17; 16]).expect("workspace id"),
            Generation::first(),
            RevisionNumber::first(),
            baseline.commit(),
            baseline.tree(),
        ),
        source.root(),
    ))
    .expect("read-only workspace");
    std::fs::write(workspace.root().join("supported.txt"), b"readable\n").expect("supported child");
    let invalid_name = std::ffi::OsStr::from_bytes(b"invalid-\xff-name");
    std::fs::write(workspace.root().join(invalid_name), b"unsupported name\n")
        .expect("non-UTF-8 child");
    symlink("tracked.txt", workspace.root().join("link")).expect("symlink child");

    let listing = workspace.list_directory(None).expect("partial listing");
    assert!(
        listing
            .entries()
            .iter()
            .any(|entry| { entry.metadata().path().as_str() == "supported.txt" })
    );
    assert!(listing.diagnostics().iter().any(|diagnostic| {
        diagnostic.kind() == DirectoryDiagnosticKind::UnsupportedName
            && diagnostic.name_bytes() == b"invalid-\xff-name"
    }));
    assert!(listing.diagnostics().iter().any(|diagnostic| {
        diagnostic.kind() == DirectoryDiagnosticKind::UnsupportedType
            && diagnostic.name_bytes() == b"link"
    }));
}
