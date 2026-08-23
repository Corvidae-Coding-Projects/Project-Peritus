//! Real Git, filesystem, patch, snapshot, and artifact conformance subject.

mod reconciliation;
mod restart;

use std::path::PathBuf;

use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_conformance::{
    WorkspaceConformanceError, WorkspaceConformanceSubject, WorkspaceMutationDisposition,
    WorkspaceMutationObservation, WorkspacePatchFixture, WorkspaceReconciliationDisposition,
    WorkspaceSnapshot,
};
use peritus_git::{
    Baseline, CandidateRequest, CandidateSnapshot, CreateWorktree, GitRepository,
    RegisteredWorktree, RepositoryOptions, RestoreRequest, SnapshotRequest, WorktreeAccess,
    WorktreeName,
};
use peritus_patch::{
    FileMode, FinalFile, LineEndingPolicy, PatchOperation, PatchSet, WorkspacePath,
};
use peritus_test_support::{FixturePath, TemporaryRepository, TemporaryRepositoryBuilder};
use peritus_types::{
    ActionId, EventId, Generation, ResourceId, RevisionNumber, Sha256Digest, SnapshotId,
    WorkspaceId,
};
use peritus_workspace::{
    ReadOnlyOpenRequest, ReadOnlyWorkspace, SnapshotIdentity, WorkspaceManifest,
};
use tempfile::TempDir;

pub struct ProductionWorkspaceSubject {
    _temp: TempDir,
    source: TemporaryRepository,
    source_head: String,
    repository: GitRepository,
    writable: RegisteredWorktree,
    read_only: ReadOnlyWorkspace,
    baseline: Baseline,
    initial: CandidateSnapshot,
    current: CandidateSnapshot,
    writable_manifest: Vec<u8>,
    read_manifest: Vec<u8>,
    initial_manifest: Vec<u8>,
    current_manifest: Vec<u8>,
    generation: Generation,
    revision: RevisionNumber,
    artifacts: ArtifactStore,
    transaction_root: PathBuf,
    manifest_finalized: bool,
    prior_candidate_retained: bool,
    next_snapshot: u8,
}

impl ProductionWorkspaceSubject {
    pub fn new() -> Result<Self, WorkspaceConformanceError> {
        let temp = TempDir::new().map_err(|_| infrastructure())?;
        let mut source = TemporaryRepositoryBuilder::new(temp.path().join("peritus-test-source"))
            .build()
            .map_err(|_| infrastructure())?;
        source
            .write_text(&FixturePath::new("README.md").map_err(|_| infrastructure())?, "base\n")
            .map_err(|_| infrastructure())?;
        let source_head =
            source.commit_all("baseline").map_err(|_| infrastructure())?.as_str().to_owned();
        let repository = GitRepository::open(RepositoryOptions::new(source.root()))
            .map_err(|error| setup_failure("open repository", &error))?;
        let baseline = repository
            .resolve_baseline("HEAD")
            .map_err(|error| setup_failure("resolve baseline", &error))?;
        let writable = repository
            .create_worktree(CreateWorktree::new(
                WorktreeName::new("writer_1").map_err(|_| infrastructure())?,
                temp.path().join("writer_1"),
                baseline,
                WorktreeAccess::Writable,
            ))
            .map_err(|error| setup_failure("create writable worktree", &error))?;
        let writable_manifest = writable
            .registration_manifest()
            .map_err(|error| setup_failure("encode writable registration", &error))?
            .bytes()
            .to_vec();
        let initial_candidate = repository
            .create_candidate(CandidateRequest::new(&writable, baseline.commit()))
            .map_err(|error| setup_failure("create initial candidate", &error))?;
        let workspace_id = workspace_id();
        let initial = repository
            .create_snapshot(SnapshotRequest::new(
                &writable,
                &initial_candidate,
                workspace_id,
                snapshot_id(1),
                baseline.commit(),
            ))
            .map_err(|error| setup_failure("create initial snapshot", &error))?;
        let initial_manifest = initial.manifest().bytes().to_vec();
        let read_registration = repository
            .create_worktree(CreateWorktree::new(
                WorktreeName::new("reviewer_1").map_err(|_| infrastructure())?,
                temp.path().join("reviewer_1"),
                initial.baseline(),
                WorktreeAccess::ReadOnly,
            ))
            .map_err(|error| setup_failure("create read-only worktree", &error))?;
        let read_manifest = read_registration
            .registration_manifest()
            .map_err(|error| setup_failure("encode read-only registration", &error))?
            .bytes()
            .to_vec();
        let read_only = ReadOnlyWorkspace::open(ReadOnlyOpenRequest::new(
            repository.clone(),
            read_registration,
            SnapshotIdentity::new(
                workspace_id,
                Generation::first(),
                RevisionNumber::first(),
                initial.commit(),
                initial.tree(),
            ),
            writable.root(),
        ))
        .map_err(|error| setup_failure("open read-only workspace", &error))?;
        let transaction_root = temp.path().join("transactions");
        std::fs::create_dir(&transaction_root).map_err(|_| infrastructure())?;
        let artifacts = ArtifactStore::open(
            StoreConfig::new(temp.path().join("artifacts"), 1_048_576, 8_388_608)
                .map_err(|_| infrastructure())?,
        )
        .map_err(|_| infrastructure())?;
        Ok(Self {
            _temp: temp,
            source,
            source_head,
            repository,
            writable,
            read_only,
            baseline,
            current: initial.clone(),
            current_manifest: initial_manifest.clone(),
            initial_manifest,
            initial,
            writable_manifest,
            read_manifest,
            generation: Generation::first(),
            revision: RevisionNumber::first(),
            artifacts,
            transaction_root,
            manifest_finalized: false,
            prior_candidate_retained: false,
            next_snapshot: 2,
        })
    }

    fn observe(&self) -> Result<WorkspaceSnapshot, WorkspaceConformanceError> {
        let first = read_optional(self.writable.root().join("src/lib.rs"))?;
        let second = read_optional(self.writable.root().join("tests/answer.rs"))?;
        let head = self
            .source
            .git_success(["rev-parse", "--verify", "refs/heads/main"])
            .map_err(|_| infrastructure())?;
        let user_ref_unchanged =
            std::str::from_utf8(head.stdout()).map_err(|_| infrastructure())?.trim_end()
                == self.source_head;
        Ok(WorkspaceSnapshot::new(
            self.generation.get(),
            self.revision.get(),
            self.current.tree().object_id().as_bytes().to_vec(),
            first,
            second,
            user_ref_unchanged,
            self.manifest_finalized,
            self.prior_candidate_retained,
        ))
    }

    fn apply_real(
        &mut self,
        fixture: &WorkspacePatchFixture,
    ) -> Result<WorkspaceMutationDisposition, WorkspaceConformanceError> {
        if fixture.resource_id() != *resource_id().as_bytes() {
            return Ok(WorkspaceMutationDisposition::Unauthorized);
        }
        if fixture.workspace_id() != *workspace_id().as_bytes()
            || fixture.generation() != self.generation.get()
            || fixture.revision() != self.revision.get()
        {
            return Ok(WorkspaceMutationDisposition::Stale);
        }
        let operations = vec![
            create_operation(fixture.first_path(), fixture.first_contents())?,
            create_operation(fixture.second_path(), fixture.second_contents())?,
        ];
        let patch = PatchSet::new(workspace_id(), self.generation, self.revision, operations)
            .map_err(|_| infrastructure())?;
        let plan = patch
            .plan(workspace_id(), self.generation, self.revision)
            .map_err(|_| infrastructure())?;
        let applied =
            peritus_patch::apply_patch(self.writable.root(), &self.transaction_root, &plan)
                .map_err(|_| infrastructure())?;
        let candidate = self
            .repository
            .create_candidate(CandidateRequest::new(&self.writable, self.baseline.commit()))
            .map_err(|_| infrastructure())?;
        let snapshot = self
            .repository
            .create_snapshot(SnapshotRequest::new(
                &self.writable,
                &candidate,
                workspace_id(),
                snapshot_id(self.next_snapshot),
                self.current.commit(),
            ))
            .map_err(|_| infrastructure())?;
        self.next_snapshot = self.next_snapshot.checked_add(1).ok_or_else(infrastructure)?;
        let next = self.revision.checked_next().map_err(|_| infrastructure())?;
        let manifest = WorkspaceManifest::candidate(
            workspace_id(),
            self.generation,
            self.revision,
            next,
            action_id(1),
            peritus_codec::sha256(applied.identity().as_bytes()),
            snapshot.tree(),
            candidate.manifest_digest(),
        );
        let finalized =
            manifest.finalize(&self.artifacts, event_id(1)).map_err(|_| infrastructure())?;
        self.artifacts.verify(finalized.digest()).map_err(|_| infrastructure())?;
        self.current_manifest = snapshot.manifest().bytes().to_vec();
        self.current = snapshot;
        self.revision = next;
        self.manifest_finalized = true;
        Ok(WorkspaceMutationDisposition::Applied)
    }
}

impl WorkspaceConformanceSubject for ProductionWorkspaceSubject {
    fn snapshot(&self) -> Result<WorkspaceSnapshot, WorkspaceConformanceError> {
        self.observe()
    }

    fn apply(
        &mut self,
        fixture: &WorkspacePatchFixture,
    ) -> Result<WorkspaceMutationObservation, WorkspaceConformanceError> {
        let disposition = self.apply_real(fixture)?;
        Ok(WorkspaceMutationObservation::new(disposition, self.observe()?))
    }

    fn apply_read_only(
        &mut self,
        _fixture: &WorkspacePatchFixture,
    ) -> Result<WorkspaceMutationObservation, WorkspaceConformanceError> {
        self.read_only.inspect().map_err(|_| infrastructure())?;
        Ok(WorkspaceMutationObservation::new(
            WorkspaceMutationDisposition::ReadOnly,
            self.observe()?,
        ))
    }

    fn rollback(&mut self) -> Result<WorkspaceSnapshot, WorkspaceConformanceError> {
        let abandoned = self.current.commit();
        self.repository
            .restore_snapshot(RestoreRequest::new(
                &self.writable,
                &self.initial,
                self.baseline.commit(),
            ))
            .map_err(|_| infrastructure())?;
        let candidate = self
            .repository
            .create_candidate(CandidateRequest::new(&self.writable, self.baseline.commit()))
            .map_err(|_| infrastructure())?;
        let snapshot = self
            .repository
            .create_snapshot(SnapshotRequest::new(
                &self.writable,
                &candidate,
                workspace_id(),
                snapshot_id(self.next_snapshot),
                abandoned,
            ))
            .map_err(|_| infrastructure())?;
        self.next_snapshot = self.next_snapshot.checked_add(1).ok_or_else(infrastructure)?;
        let next = self.revision.checked_next().map_err(|_| infrastructure())?;
        let manifest = WorkspaceManifest::rollback(
            workspace_id(),
            self.generation,
            self.revision,
            next,
            action_id(2),
            Sha256Digest::new([2; 32]),
            snapshot.tree(),
            candidate.manifest_digest(),
        );
        let finalized =
            manifest.finalize(&self.artifacts, event_id(2)).map_err(|_| infrastructure())?;
        self.artifacts.verify(finalized.digest()).map_err(|_| infrastructure())?;
        self.prior_candidate_retained =
            self.source.git_success(["cat-file", "-e", &abandoned.to_string()]).is_ok();
        self.current_manifest = snapshot.manifest().bytes().to_vec();
        self.current = snapshot;
        self.revision = next;
        self.manifest_finalized = true;
        self.observe()
    }

    fn restart(&mut self) -> Result<(), WorkspaceConformanceError> {
        restart::restart(self)
    }

    fn make_dirty(&mut self) -> Result<(), WorkspaceConformanceError> {
        std::fs::write(self.writable.root().join("dirty.txt"), b"dirty\n")
            .map_err(|_| infrastructure())
    }

    fn make_indeterminate(&mut self) -> Result<(), WorkspaceConformanceError> {
        std::fs::write(self.writable.git_dir().join("index"), b"not-a-git-index")
            .map_err(|_| infrastructure())
    }

    fn reconcile(
        &mut self,
        expected_generation: u64,
    ) -> Result<WorkspaceReconciliationDisposition, WorkspaceConformanceError> {
        reconciliation::reconcile(self, expected_generation)
    }
}

fn create_operation(
    path: &str,
    contents: &[u8],
) -> Result<PatchOperation, WorkspaceConformanceError> {
    Ok(PatchOperation::create(
        WorkspacePath::new(path).map_err(|_| infrastructure())?,
        FinalFile::new(contents.to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
            .map_err(|_| infrastructure())?,
    ))
}

fn read_optional(path: PathBuf) -> Result<Option<Vec<u8>>, WorkspaceConformanceError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(infrastructure()),
    }
}

const fn infrastructure() -> WorkspaceConformanceError {
    WorkspaceConformanceError::Infrastructure
}

const fn setup_failure(_stage: &str, _error: &impl std::fmt::Debug) -> WorkspaceConformanceError {
    infrastructure()
}

fn workspace_id() -> WorkspaceId {
    WorkspaceId::new([1; 16]).expect("workspace")
}
fn resource_id() -> ResourceId {
    ResourceId::new([2; 16]).expect("resource")
}
fn action_id(seed: u8) -> ActionId {
    ActionId::new([seed; 16]).expect("action")
}
fn event_id(seed: u8) -> EventId {
    EventId::new([seed; 16]).expect("event")
}
fn snapshot_id(seed: u8) -> SnapshotId {
    SnapshotId::new([seed; 16]).expect("snapshot")
}
