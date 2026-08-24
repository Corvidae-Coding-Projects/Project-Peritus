use peritus_git::{CreateWorktree, GitRepository, RepositoryOptions, WorktreeAccess, WorktreeName};
use peritus_test_support::{FixturePath, TemporaryRepository, TemporaryRepositoryBuilder};
use peritus_types::{EnvironmentId, Generation, ResourceId, RevisionNumber, WorkspaceId};
use peritus_workspace::{
    ReadOnlyOpenRequest, ReadOnlyWorkspace, SnapshotIdentity, WorkspaceBinding,
};
use tempfile::TempDir;

pub struct GitFixture {
    pub workspace: ReadOnlyWorkspace,
    pub first_commit: String,
    pub source: TemporaryRepository,
    pub _temp: TempDir,
}

pub fn git_fixture(label: &str) -> GitFixture {
    let temp = TempDir::new().expect("temporary root");
    let mut source =
        TemporaryRepositoryBuilder::new(temp.path().join(format!("peritus-test-{label}-source")))
            .build()
            .expect("temporary repository");
    source
        .write_text(&FixturePath::new("README.md").expect("path"), "first\n")
        .expect("first README");
    let first_commit = source.commit_all("first").expect("first commit").as_str().to_owned();
    source
        .write_text(&FixturePath::new("README.md").expect("path"), "second\n")
        .expect("second README");
    source
        .write_text(
            &FixturePath::new("src/main.rs").expect("path"),
            "fn main() { println!(\"ok\"); }\n",
        )
        .expect("source file");
    source.commit_all("second").expect("second commit");

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
    let workspace_id = WorkspaceId::new([51; 16]).expect("workspace ID");
    let binding = WorkspaceBinding::new(
        workspace_id,
        ResourceId::new([52; 16]).expect("resource ID"),
        EnvironmentId::new([53; 16]).expect("environment ID"),
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
    GitFixture { workspace, first_commit, source, _temp: temp }
}
