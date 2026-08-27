//! Canonical durable workspace registration and recovery handoff tests.

use peritus_git::{CreateWorktree, GitRepository, RepositoryOptions, WorktreeAccess, WorktreeName};
use peritus_journal::{NewApplicationWorkspace, SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_test_support::{FixturePath, TemporaryRepositoryBuilder};
use peritus_types::{EnvironmentId, ResourceId, WorkspaceId};
use peritus_workspace::{ErrorCode, WorkspaceBinding, WorkspaceRegistration};

#[test]
fn workspace_registration_round_trips_exact_reopen_inputs() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let mut source =
        TemporaryRepositoryBuilder::new(temp.path().join("peritus-test-registration-source"))
            .build()
            .expect("source repository");
    source
        .write_text(&FixturePath::new("README.md").expect("fixture path"), "baseline\n")
        .expect("baseline file");
    source.commit_all("baseline").expect("baseline commit");
    let repository =
        GitRepository::open(RepositoryOptions::new(source.root())).expect("checked repository");
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("registration_writer").expect("worktree name"),
            temp.path().join("registration_writer"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("writable worktree");
    let workspace_id = WorkspaceId::new([1; 16]).expect("workspace identity");
    let resource_id = ResourceId::new([2; 16]).expect("resource identity");
    let environment_id = EnvironmentId::new([3; 16]).expect("environment identity");
    let binding = WorkspaceBinding::new(
        workspace_id,
        resource_id,
        environment_id,
        worktree.root().to_owned(),
        baseline.commit(),
        baseline.tree(),
    )
    .expect("workspace binding");
    let registration = WorkspaceRegistration::new(
        &binding,
        &repository,
        &worktree,
        temp.path().join("transactions"),
    )
    .expect("workspace registration");

    let decoded = WorkspaceRegistration::decode(registration.canonical_bytes())
        .expect("canonical round trip");
    assert_eq!(decoded, registration);
    assert_eq!(decoded.workspace_id(), workspace_id);
    assert_eq!(decoded.resource_id(), resource_id);
    assert_eq!(decoded.environment_id(), environment_id);
    assert_eq!(decoded.workspace_binding().expect("decoded binding"), binding);
    assert_eq!(decoded.digest(), peritus_codec::sha256(decoded.canonical_bytes()));
    let recovered_repository =
        GitRepository::open(RepositoryOptions::new(decoded.repository_root()))
            .expect("recover repository");
    let recovered_worktree = recovered_repository
        .reopen_worktree(decoded.worktree_manifest())
        .expect("recover exact writable worktree");
    assert_eq!(recovered_worktree.root(), binding.root());
    let durable = decoded.durable_registration().expect("durable catalog value");
    assert_eq!(durable, decoded.durable_registration().expect("stable durable catalog value"),);
    let mut journal = SqliteJournal::open(
        temp.path().join("journal.sqlite3"),
        StoreId::new([7; 16]).expect("store identity"),
        SqliteJournalOptions::default(),
    )
    .expect("journal");
    let row = journal.register_application_workspace(durable).expect("durable registration");
    assert_eq!(
        WorkspaceRegistration::from_application_workspace(&row).expect("checked durable recovery"),
        decoded,
    );
    let wrong_workspace = WorkspaceId::new([8; 16]).expect("wrong workspace identity");
    let wrong = NewApplicationWorkspace::new(
        wrong_workspace,
        decoded.canonical_bytes().to_vec(),
        decoded.digest(),
    )
    .expect("outer mismatch is structurally bounded");
    let wrong_row = journal.register_application_workspace(wrong).expect("mismatched row");
    assert_eq!(
        WorkspaceRegistration::from_application_workspace(&wrong_row)
            .expect_err("outer workspace mismatch must fail")
            .code(),
        ErrorCode::InvalidInput,
    );

    let mut trailing = decoded.canonical_bytes().to_vec();
    trailing.push(0);
    assert_eq!(
        WorkspaceRegistration::decode(&trailing).expect_err("trailing bytes must fail").code(),
        ErrorCode::InvalidInput,
    );
}

#[test]
fn registration_rejects_malformed_bytes_and_read_only_worktrees() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let mut source = TemporaryRepositoryBuilder::new(
        temp.path().join("peritus-test-registration-readonly-source"),
    )
    .build()
    .expect("source repository");
    source
        .write_text(&FixturePath::new("README.md").expect("fixture path"), "baseline\n")
        .expect("baseline file");
    source.commit_all("baseline").expect("baseline commit");
    let repository =
        GitRepository::open(RepositoryOptions::new(source.root())).expect("checked repository");
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("registration_reader").expect("worktree name"),
            temp.path().join("registration_reader"),
            baseline,
            WorktreeAccess::ReadOnly,
        ))
        .expect("read-only worktree");
    let binding = WorkspaceBinding::new(
        WorkspaceId::new([4; 16]).expect("workspace identity"),
        ResourceId::new([5; 16]).expect("resource identity"),
        EnvironmentId::new([6; 16]).expect("environment identity"),
        worktree.root().to_owned(),
        baseline.commit(),
        baseline.tree(),
    )
    .expect("workspace binding");
    let error = WorkspaceRegistration::new(
        &binding,
        &repository,
        &worktree,
        temp.path().join("transactions"),
    )
    .expect_err("read-only registration must fail");
    assert_eq!(error.code(), ErrorCode::InvalidInput);

    let mut bytes = b"not-a-registration".to_vec();
    bytes.push(0);
    assert_eq!(
        WorkspaceRegistration::decode(&bytes).expect_err("malformed registration must fail").code(),
        ErrorCode::InvalidInput,
    );
}
