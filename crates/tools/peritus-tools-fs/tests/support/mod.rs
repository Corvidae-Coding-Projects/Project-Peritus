use peritus_git::{CreateWorktree, GitRepository, RepositoryOptions, WorktreeAccess, WorktreeName};
use peritus_test_support::{FixturePath, TemporaryRepository, TemporaryRepositoryBuilder};
use peritus_types::{EnvironmentId, Generation, ResourceId, RevisionNumber, WorkspaceId};
use peritus_workspace::{
    ReadOnlyOpenRequest, ReadOnlyWorkspace, SnapshotIdentity, WorkspaceBinding,
};
use tempfile::TempDir;

pub struct ReadFixture {
    pub workspace: ReadOnlyWorkspace,
    #[cfg(unix)]
    pub root: std::path::PathBuf,
    pub _source: TemporaryRepository,
    pub _temp: TempDir,
}

pub fn read_fixture(label: &str) -> ReadFixture {
    let temp = TempDir::new().expect("temporary root");
    let mut source =
        TemporaryRepositoryBuilder::new(temp.path().join(format!("peritus-test-{label}-source")))
            .build()
            .expect("temporary repository");
    source
        .write_text(&FixturePath::new("README.md").expect("path"), "Alpha\nbeta\n")
        .expect("README");
    source
        .write_text(&FixturePath::new("src/lib.rs").expect("path"), "pub fn alpha() -> u8 { 1 }\n")
        .expect("Rust source");
    std::fs::write(source.root().join("blob.bin"), [0, 1, 2, 255]).expect("binary fixture");
    source.commit_all("immutable snapshot").expect("snapshot commit");

    let repository = GitRepository::open(RepositoryOptions::new(source.root())).expect("open Git");
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let writer = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new(format!("{label}_writer")).expect("writer name"),
            temp.path().join(format!("{label}_writer")),
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("writer worktree");
    let reader = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new(format!("{label}_reader")).expect("reader name"),
            temp.path().join(format!("{label}_reader")),
            baseline,
            WorktreeAccess::ReadOnly,
        ))
        .expect("reader worktree");
    let workspace_id = WorkspaceId::new([31; 16]).expect("workspace ID");
    let binding = WorkspaceBinding::new(
        workspace_id,
        ResourceId::new([32; 16]).expect("resource ID"),
        EnvironmentId::new([33; 16]).expect("environment ID"),
        writer.root().to_owned(),
        baseline.commit(),
        baseline.tree(),
    )
    .expect("workspace binding");
    let workspace = ReadOnlyWorkspace::open(
        ReadOnlyOpenRequest::new(
            repository,
            reader,
            SnapshotIdentity::new(
                workspace_id,
                Generation::first(),
                RevisionNumber::first(),
                baseline.commit(),
                baseline.tree(),
            ),
            writer.root(),
        )
        .with_workspace_binding(binding),
    )
    .expect("read-only workspace");
    #[cfg(unix)]
    let root = workspace.root().to_owned();
    ReadFixture {
        workspace,
        #[cfg(unix)]
        root,
        _source: source,
        _temp: temp,
    }
}
