use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_git::{
    CandidateRequest, CandidateSnapshot, CandidateSnapshotManifest, CreateWorktree, GitRepository,
    RepositoryOptions, SnapshotRequest, WorktreeAccess, WorktreeName, WorktreeRegistrationManifest,
};
use peritus_patch::{
    FileMode, FinalFile, LineEndingPolicy, PatchOperation, PatchSet, Preimage, WorkspacePath,
};
use peritus_policy::{ActorRole, OperationClass};
use peritus_protocol::ActionIntentDto;
use peritus_test_support::{FixturePath, TemporaryRepository, TemporaryRepositoryBuilder};
use peritus_types::{Generation, RevisionNumber, SnapshotId};
use peritus_workspace::{
    MutationOutcome, SnapshotIdentity, WorkspaceAuthorizationRequest, WorkspaceBinding,
    WorkspaceCondition, WorkspaceGateway, WorkspaceState, WritableOpenRequest, WritableWorkspace,
    patch_authorization_payload,
};
use tempfile::TempDir;

use super::{AuthorityReceipts, Ids, commit_authority, open_journal};

pub struct WorkspaceFixture {
    pub _source: TemporaryRepository,
    pub gateway: WorkspaceGateway,
    pub patch: PatchSet,
    pub initial: CandidateSnapshot,
    pub persistence: ReopenFixture,
}

#[derive(Clone)]
pub struct ReopenFixture {
    source_root: std::path::PathBuf,
    worktree_manifest: Vec<u8>,
    snapshot_manifest: Vec<u8>,
    transaction_root: std::path::PathBuf,
}

pub fn workspace_fixture(temp: &TempDir, ids: &Ids, label: &str) -> WorkspaceFixture {
    let owned_root = temp.path().join(format!("peritus-test-{label}-source"));
    let mut source =
        TemporaryRepositoryBuilder::new(&owned_root).build().expect("source repository");
    let source_root = source.root().to_owned();
    source
        .write_text(&FixturePath::new("README.md").expect("path"), "baseline\n")
        .expect("baseline file");
    source.commit_all("baseline").expect("baseline commit");
    let repository =
        GitRepository::open(RepositoryOptions::new(source.root())).expect("open repository");
    let baseline = repository.resolve_baseline("HEAD").expect("baseline");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new(format!("{label}_writer")).expect("worktree name"),
            temp.path().join(format!("{label}_writer")),
            baseline,
            WorktreeAccess::Writable,
        ))
        .expect("writable worktree");
    let worktree_manifest =
        worktree.registration_manifest().expect("worktree registration manifest").bytes().to_vec();
    let candidate = repository
        .create_candidate(CandidateRequest::new(&worktree, baseline.commit()))
        .expect("initial candidate");
    let snapshot = repository
        .create_snapshot(SnapshotRequest::new(
            &worktree,
            &candidate,
            ids.workspace,
            SnapshotId::new([80; 16]).expect("snapshot"),
            baseline.commit(),
        ))
        .expect("initial snapshot");
    let snapshot_manifest = snapshot.manifest().bytes().to_vec();
    let binding = WorkspaceBinding::new(
        ids.workspace,
        ids.resource,
        ids.environment,
        worktree.root().to_owned(),
        baseline.commit(),
        baseline.tree(),
    )
    .expect("workspace binding");
    let state = WorkspaceState::new(
        binding,
        Generation::first(),
        RevisionNumber::first(),
        SnapshotIdentity::new(
            ids.workspace,
            Generation::first(),
            RevisionNumber::first(),
            snapshot.commit(),
            snapshot.tree(),
        ),
        ids.holder(),
        WorkspaceCondition::Clean,
    )
    .expect("workspace state");
    let transaction_root = temp.path().join(format!("{label}-transactions"));
    let workspace = WritableWorkspace::open(WritableOpenRequest::new(
        repository,
        worktree,
        state,
        &transaction_root,
    ))
    .expect("writable workspace");
    let operation = PatchOperation::create(
        WorkspacePath::new("authorized.txt").expect("workspace path"),
        FinalFile::new(b"authorized\n".to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
            .expect("final file"),
    );
    let patch =
        PatchSet::new(ids.workspace, Generation::first(), RevisionNumber::first(), vec![operation])
            .expect("patch set");
    WorkspaceFixture {
        _source: source,
        gateway: WorkspaceGateway::new(workspace),
        patch,
        initial: snapshot,
        persistence: ReopenFixture {
            source_root,
            worktree_manifest,
            snapshot_manifest,
            transaction_root,
        },
    }
}

pub fn intent(ids: &Ids, payload: Vec<u8>) -> ActionIntentDto {
    ActionIntentDto {
        action_id: ids.action,
        actor_id: ids.actor,
        role: ActorRole::Writer,
        environment_id: ids.environment,
        resource_id: ids.resource,
        capability_name: ids.capability.clone(),
        operation_class: OperationClass::WorkspaceMutation,
        media_type: "application/vnd.peritus.workspace-operation.v1".to_owned(),
        payload,
    }
}

pub fn receipts(temp: &TempDir, ids: &Ids, intent: &ActionIntentDto) -> AuthorityReceipts {
    let journal_root = TempDir::new_in(temp.path()).expect("authority journal root");
    let mut journal = open_journal(&journal_root);
    commit_authority(&mut journal, ids, intent)
}

pub fn artifact_store(temp: &TempDir, name: &str, max_artifact_bytes: u64) -> ArtifactStore {
    ArtifactStore::open(
        StoreConfig::new(temp.path().join(name), max_artifact_bytes, 8_388_608)
            .expect("artifact configuration"),
    )
    .expect("artifact store")
}

pub fn authorized_patch(
    temp: &TempDir,
    ids: &Ids,
    gateway: &mut WorkspaceGateway,
    patch: PatchSet,
) -> MutationOutcome {
    let action = intent(ids, patch_authorization_payload(&patch));
    let committed = receipts(temp, ids, &action);
    let request = exact_request(&action, &committed, ids);
    gateway.apply_patch(&request, patch).expect("authorized patch")
}

pub fn mismatched_preimage_patch(ids: &Ids) -> PatchSet {
    let operation = PatchOperation::replace(
        WorkspacePath::new("README.md").expect("workspace path"),
        Preimage::from_bytes(b"not the baseline\n", FileMode::Regular),
        FinalFile::new(b"replacement\n".to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
            .expect("final file"),
    )
    .expect("replacement operation");
    PatchSet::new(ids.workspace, Generation::first(), RevisionNumber::first(), vec![operation])
        .expect("mismatched preimage patch")
}

pub fn reopen_fixture(persistence: &ReopenFixture, ids: &Ids) -> WorkspaceGateway {
    try_reopen_fixture(persistence, ids).expect("reopen writable workspace")
}

pub fn try_reopen_fixture(
    persistence: &ReopenFixture,
    ids: &Ids,
) -> Result<WorkspaceGateway, peritus_workspace::WorkspaceError> {
    try_reopen_fixture_at(persistence, ids, &persistence.transaction_root)
}

pub fn try_reopen_fixture_at(
    persistence: &ReopenFixture,
    ids: &Ids,
    transaction_root: &std::path::Path,
) -> Result<WorkspaceGateway, peritus_workspace::WorkspaceError> {
    let repository = GitRepository::open(RepositoryOptions::new(&persistence.source_root))
        .expect("reopen repository");
    let worktree_manifest = WorktreeRegistrationManifest::decode(&persistence.worktree_manifest)
        .expect("decode worktree registration");
    let snapshot_manifest = CandidateSnapshotManifest::decode(&persistence.snapshot_manifest)
        .expect("decode snapshot registration");
    let worktree = repository.reopen_worktree(&worktree_manifest).expect("reopen worktree");
    let snapshot = repository.reopen_snapshot(&snapshot_manifest).expect("reopen snapshot");
    let baseline = worktree.baseline();
    let binding = WorkspaceBinding::new(
        ids.workspace,
        ids.resource,
        ids.environment,
        worktree.root().to_owned(),
        baseline.commit(),
        baseline.tree(),
    )
    .expect("workspace binding");
    let state = WorkspaceState::new(
        binding,
        Generation::first(),
        RevisionNumber::first(),
        SnapshotIdentity::new(
            ids.workspace,
            Generation::first(),
            RevisionNumber::first(),
            snapshot.commit(),
            snapshot.tree(),
        ),
        ids.holder(),
        WorkspaceCondition::Clean,
    )
    .expect("workspace state");
    let workspace = WritableWorkspace::open(WritableOpenRequest::new(
        repository,
        worktree,
        state,
        transaction_root,
    ))?;
    Ok(WorkspaceGateway::new(workspace))
}

const fn exact_request<'a>(
    intent: &'a ActionIntentDto,
    receipts: &'a AuthorityReceipts,
    ids: &Ids,
) -> WorkspaceAuthorizationRequest<'a> {
    WorkspaceAuthorizationRequest::new(
        intent,
        &receipts.kernel,
        &receipts.capability,
        &receipts.lease,
        &receipts.epoch,
        ids.revision,
        ids.session,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        receipts.observed_at,
    )
}
