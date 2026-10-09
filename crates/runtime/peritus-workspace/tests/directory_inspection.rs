//! Child-local inspection diagnostics preserve siblings and exact raw names.

#![cfg(unix)]

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
    #[cfg(not(target_os = "macos"))]
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::symlink;

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
    #[cfg(not(target_os = "macos"))]
    {
        let invalid_name = std::ffi::OsStr::from_bytes(b"invalid-\xff-name");
        std::fs::write(workspace.root().join(invalid_name), b"unsupported name\n")
            .expect("non-UTF-8 child");
    }
    symlink("tracked.txt", workspace.root().join("link")).expect("symlink child");

    let listing = workspace.list_directory(None).expect("partial listing");
    assert!(
        listing
            .entries()
            .iter()
            .any(|entry| { entry.metadata().path().as_str() == "supported.txt" })
    );
    #[cfg(not(target_os = "macos"))]
    assert!(listing.diagnostics().iter().any(|diagnostic| {
        diagnostic.kind() == DirectoryDiagnosticKind::UnsupportedName
            && diagnostic.name_bytes() == b"invalid-\xff-name"
    }));
    assert!(listing.diagnostics().iter().any(|diagnostic| {
        diagnostic.kind() == DirectoryDiagnosticKind::UnsupportedType
            && diagnostic.name_bytes() == b"link"
    }));
}

#[cfg(unix)]
#[test]
fn paged_listing_bounds_and_reports_each_unsupported_child_once() {
    #[cfg(not(target_os = "macos"))]
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::symlink;

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
    for index in 0..260 {
        std::fs::write(workspace.root().join(format!("file-{index:03}")), b"x")
            .expect("supported child");
        symlink("tracked.txt", workspace.root().join(format!("link-{index:03}")))
            .expect("unsupported child");
    }
    #[cfg(not(target_os = "macos"))]
    std::fs::write(workspace.root().join(std::ffi::OsStr::from_bytes(b"invalid-\xff")), b"x")
        .expect("non-UTF-8 child");
    let mut cursor = None;
    let mut names = std::collections::BTreeSet::new();
    let mut supported = 0;
    let mut pages = 0;
    loop {
        let page = workspace
            .list_directory_page_cancellable(None, cursor.as_ref(), || false)
            .expect("page")
            .expect("not canceled");
        assert!(page.entries().len() + page.diagnostics().len() <= 128);
        supported += page.entries().len();
        for diagnostic in page.diagnostics() {
            assert!(names.insert(diagnostic.name_bytes().to_vec()), "diagnostic repeated");
        }
        pages += 1;
        cursor = page.next_cursor().cloned();
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(supported, 261);
    #[cfg(target_os = "macos")]
    let expected_diagnostics = 260;
    #[cfg(not(target_os = "macos"))]
    let expected_diagnostics = 261;
    assert_eq!(names.len(), expected_diagnostics);
    assert_eq!(pages, 5);
    #[cfg(not(target_os = "macos"))]
    assert!(names.contains(b"invalid-\xff".as_slice()));
}
