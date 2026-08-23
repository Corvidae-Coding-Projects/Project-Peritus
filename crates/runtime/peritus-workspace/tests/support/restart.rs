use peritus_conformance::WorkspaceConformanceError;
use peritus_git::{
    CandidateSnapshotManifest, GitRepository, RepositoryOptions, WorktreeRegistrationManifest,
};
use peritus_types::{Generation, RevisionNumber};
use peritus_workspace::{ReadOnlyOpenRequest, ReadOnlyWorkspace, SnapshotIdentity};

use super::{ProductionWorkspaceSubject, infrastructure, workspace_id};

pub(super) fn restart(
    subject: &mut ProductionWorkspaceSubject,
) -> Result<(), WorkspaceConformanceError> {
    let repository = GitRepository::open(RepositoryOptions::new(subject.source.root()))
        .map_err(|_| infrastructure())?;
    let writable_manifest = WorktreeRegistrationManifest::decode(&subject.writable_manifest)
        .map_err(|_| infrastructure())?;
    let read_manifest = WorktreeRegistrationManifest::decode(&subject.read_manifest)
        .map_err(|_| infrastructure())?;
    let initial_manifest = CandidateSnapshotManifest::decode(&subject.initial_manifest)
        .map_err(|_| infrastructure())?;
    let current_manifest = CandidateSnapshotManifest::decode(&subject.current_manifest)
        .map_err(|_| infrastructure())?;
    let writable = repository.reopen_worktree(&writable_manifest).map_err(|_| infrastructure())?;
    let read_registration =
        repository.reopen_worktree(&read_manifest).map_err(|_| infrastructure())?;
    let initial = repository.reopen_snapshot(&initial_manifest).map_err(|_| infrastructure())?;
    let current = repository.reopen_snapshot(&current_manifest).map_err(|_| infrastructure())?;
    let baseline = repository
        .resolve_baseline(&subject.baseline.commit().to_string())
        .map_err(|_| infrastructure())?;
    let read_only = ReadOnlyWorkspace::open(ReadOnlyOpenRequest::new(
        repository.clone(),
        read_registration,
        SnapshotIdentity::new(
            workspace_id(),
            Generation::first(),
            RevisionNumber::first(),
            initial.commit(),
            initial.tree(),
        ),
        writable.root(),
    ))
    .map_err(|_| infrastructure())?;
    subject.repository = repository;
    subject.writable = writable;
    subject.read_only = read_only;
    subject.baseline = baseline;
    subject.initial = initial;
    subject.current = current;
    Ok(())
}
