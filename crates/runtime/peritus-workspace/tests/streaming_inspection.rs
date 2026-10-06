//! Immutable snapshot inspection uses the same streaming and retained-page contracts.

use peritus_git::{CreateWorktree, GitRepository, RepositoryOptions, WorktreeAccess, WorktreeName};
use peritus_patch::WorkspacePath;
use peritus_test_support::TemporaryRepositoryBuilder;
use peritus_types::{Generation, RevisionNumber, WorkspaceId};
use peritus_workspace::{
    FileReadSelection, ReadOnlyOpenRequest, ReadOnlyWorkspace, SnapshotIdentity,
};

#[test]
fn immutable_snapshot_reads_above_former_buffer_ceiling_and_retains_exact_selection() {
    let root = tempfile::tempdir().unwrap();
    let source =
        TemporaryRepositoryBuilder::new(root.path().join("peritus-test-inspection-source"))
            .build()
            .unwrap();
    let bytes = vec![b'x'; 8 * 1024 * 1024 + 1];
    std::fs::write(source.root().join("large"), &bytes).unwrap();
    source.commit_all("retained inspection fixture").unwrap();
    let repository = GitRepository::open(RepositoryOptions::new(source.root())).unwrap();
    let baseline = repository.resolve_baseline("HEAD").unwrap();
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("inspection_reader").unwrap(),
            root.path().join("inspection_reader"),
            baseline,
            WorktreeAccess::ReadOnly,
        ))
        .unwrap();
    let snapshot = SnapshotIdentity::new(
        WorkspaceId::new([2; 16]).unwrap(),
        Generation::first(),
        RevisionNumber::first(),
        baseline.commit(),
        baseline.tree(),
    );
    let read_only = ReadOnlyWorkspace::open(ReadOnlyOpenRequest::new(
        repository,
        worktree,
        snapshot,
        root.path().join("separate_writer"),
    ))
    .unwrap();
    let path = WorkspacePath::new("large").unwrap();
    assert_eq!(read_only.read_file(&path, bytes.len() as u64).unwrap(), bytes);
    let selection = FileReadSelection::bytes(8 * 1024 * 1024, bytes.len() as u64).unwrap();
    let mut output = Vec::new();
    let observed = read_only.copy_selection(&path, selection, &mut output).unwrap();
    assert_eq!(output, b"x");
    assert_eq!(observed.range(), (8 * 1024 * 1024, bytes.len() as u64));
    assert_eq!(observed.source_digest(), peritus_codec::sha256(&bytes));
    let mut retained =
        read_only.capture_file(&path, selection, tempfile::tempfile().unwrap()).unwrap();
    let page = retained.read_page(retained.cursor().unwrap(), u64::MAX).unwrap().unwrap();
    assert_eq!(page.observation(), &observed);
    assert_eq!(page.bytes(), b"x");
    assert!(page.next().is_none());
    assert!(read_only.inspect().unwrap().is_clean());
}
