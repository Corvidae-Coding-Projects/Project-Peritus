//! Git effects for candidates, snapshot references, and restoration.

use std::ffi::OsString;

use crate::command::{CommandAccess, one_line};
use crate::repository::strings;
use crate::{
    CommitId, ErrorKind, GitError, GitRepository, ObjectId, Operation, RecoveryClass,
    RegisteredWorktree, TreeId, WorktreeAccess,
};

use super::support::{
    object_mismatch, reject_nested_git_metadata, retain_ref, snapshot_ref,
    validate_candidate_binding, verify_retained,
};
use super::{
    CandidateRequest, CandidateSnapshot, CandidateTree, RestoreObservation, RestoreRequest,
    SnapshotRequest,
};

impl GitRepository {
    /// Stages the complete isolated worktree, writes its tree, and returns a canonical manifest.
    ///
    /// # Errors
    ///
    /// Rejects registration or HEAD drift, nested repository metadata, Git staging failures, and
    /// an index tree that cannot be validated under the repository object format.
    pub fn create_candidate(
        &self,
        request: CandidateRequest<'_>,
    ) -> Result<CandidateTree, GitError> {
        self.validate_registration(request.worktree, Operation::CreateCandidate)?;
        require_writable(request.worktree, Operation::CreateCandidate)?;
        let observed = self.inspect_worktree(request.worktree)?;
        if observed.head() != request.expected_head || !observed.is_detached() {
            return Err(object_mismatch(
                Operation::CreateCandidate,
                "worktree HEAD changed or is no longer detached",
            ));
        }
        reject_nested_git_metadata(request.worktree.root(), Operation::CreateCandidate)?;
        let prior_status = self.status(request.worktree)?;
        self.runner.checked(
            request.worktree.root(),
            Some(Self::worktree_location(request.worktree.root(), request.worktree.git_dir())),
            CommandAccess::Write,
            Operation::CreateCandidate,
            &strings(&["add", "--all", "--", "."]),
            None,
        )?;
        let tree = self.write_tree(request.worktree, Operation::CreateCandidate)?;
        let status = self.status(request.worktree)?;
        if status.head() != request.expected_head
            || !status.is_detached()
            || status.index_tree() != Some(tree)
        {
            return Err(object_mismatch(
                Operation::CreateCandidate,
                "candidate observation changed during creation",
            ));
        }
        let manifest = crate::manifest::candidate_manifest(
            self.identity.digest(),
            request.worktree.root(),
            request.worktree.baseline(),
            request.expected_head,
            tree,
            prior_status.digest(),
            status.digest(),
        )?;
        Ok(CandidateTree {
            repository_digest: self.identity.digest(),
            worktree_root: request.worktree.root().to_owned(),
            baseline: request.worktree.baseline(),
            head: request.expected_head,
            tree,
            manifest,
            prior_status,
            status,
        })
    }

    /// Creates a deterministic synthetic commit and retains it under a C1-owned reference.
    ///
    /// This operation does not move the worktree HEAD or rewrite its index.
    ///
    /// # Errors
    ///
    /// Rejects candidate drift, a missing parent, or a reference already bound to another commit.
    pub fn create_snapshot(
        &self,
        request: SnapshotRequest<'_>,
    ) -> Result<CandidateSnapshot, GitError> {
        validate_candidate_binding(self, request.worktree, request.candidate)?;
        let current = self.status(request.worktree)?;
        if current.head() != request.candidate.head()
            || !current.is_detached()
            || current.index_tree() != Some(request.candidate.tree())
            || current.digest() != request.candidate.status().digest()
        {
            return Err(object_mismatch(
                Operation::CreateSnapshot,
                "candidate index or status changed before snapshot creation",
            ));
        }
        self.verify_commit(request.parent, Operation::CreateSnapshot)?;
        let reference = snapshot_ref(request.workspace_id, request.snapshot_id);
        let message = format!(
            "Peritus snapshot {} for workspace {}\n",
            super::support::identifier_hex(request.snapshot_id.as_bytes()),
            super::support::identifier_hex(request.workspace_id.as_bytes())
        );
        let arguments = vec![
            OsString::from("commit-tree"),
            OsString::from(request.candidate.tree().to_string()),
            OsString::from("-p"),
            OsString::from(request.parent.to_string()),
        ];
        let output = self.checked_repo_command(
            Operation::CreateSnapshot,
            CommandAccess::Write,
            &arguments,
            Some(message.as_bytes()),
        )?;
        let commit = CommitId::checked(ObjectId::parse(
            self.identity.object_format(),
            one_line(&output.stdout, Operation::CreateSnapshot)?,
            Operation::CreateSnapshot,
        )?);
        retain_ref(self, &reference, commit)?;
        let manifest = crate::manifest::snapshot_manifest(
            self.identity.digest(),
            request.workspace_id,
            request.snapshot_id,
            request.parent,
            commit,
            request.candidate.tree(),
            reference,
            request.candidate.manifest_digest(),
        )?;
        Ok(CandidateSnapshot { manifest })
    }

    /// Reopens a persisted snapshot after revalidating its repository, commit graph, tree, and
    /// retained reference.
    ///
    /// # Errors
    ///
    /// Rejects repository identity, deterministic reference, commit, parent, tree, or ref drift.
    pub fn reopen_snapshot(
        &self,
        manifest: &crate::CandidateSnapshotManifest,
    ) -> Result<CandidateSnapshot, GitError> {
        if manifest.repository_digest() != self.identity.digest() {
            return Err(snapshot_mismatch("snapshot manifest belongs to another repository"));
        }
        let expected_ref = snapshot_ref(manifest.workspace_id(), manifest.snapshot_id());
        if &expected_ref != manifest.reference() {
            return Err(snapshot_mismatch("snapshot manifest reference is not canonical"));
        }
        let baseline = self.resolve_baseline(&manifest.commit().to_string())?;
        if baseline.commit() != manifest.commit() || baseline.tree() != manifest.tree() {
            return Err(snapshot_mismatch("snapshot commit or tree no longer resolves exactly"));
        }
        let parent = self.snapshot_parent(manifest.commit())?;
        if parent != manifest.parent() {
            return Err(snapshot_mismatch("snapshot commit parent differs from its manifest"));
        }
        let snapshot = CandidateSnapshot { manifest: manifest.clone() };
        verify_retained(self, &snapshot, Operation::ReopenSnapshot)?;
        Ok(snapshot)
    }

    /// Restores a retained snapshot tree into the exact registered worktree and index.
    ///
    /// The detached HEAD and retained snapshot history are not rewritten.
    ///
    /// # Errors
    ///
    /// Rejects repository, reference, or HEAD drift and reports any non-exact restored tree.
    pub fn restore_snapshot(
        &self,
        request: RestoreRequest<'_>,
    ) -> Result<RestoreObservation, GitError> {
        self.validate_registration(request.worktree, Operation::RestoreSnapshot)?;
        require_writable(request.worktree, Operation::RestoreSnapshot)?;
        if request.snapshot.manifest.repository_digest() != self.identity.digest() {
            return Err(object_mismatch(
                Operation::RestoreSnapshot,
                "snapshot belongs to another repository",
            ));
        }
        verify_retained(self, request.snapshot, Operation::RestoreSnapshot)?;
        let observed = self.inspect_worktree(request.worktree)?;
        if observed.head() != request.expected_head || !observed.is_detached() {
            return Err(object_mismatch(
                Operation::RestoreSnapshot,
                "worktree HEAD changed or is no longer detached",
            ));
        }
        reject_nested_git_metadata(request.worktree.root(), Operation::RestoreSnapshot)?;
        let prior_tree = self.status(request.worktree)?.index_tree();
        let arguments = vec![
            OsString::from("read-tree"),
            OsString::from("--reset"),
            OsString::from("-u"),
            OsString::from(request.snapshot.tree().to_string()),
        ];
        self.runner.checked(
            request.worktree.root(),
            Some(Self::worktree_location(request.worktree.root(), request.worktree.git_dir())),
            CommandAccess::Write,
            Operation::RestoreSnapshot,
            &arguments,
            None,
        )?;
        self.runner.checked(
            request.worktree.root(),
            Some(Self::worktree_location(request.worktree.root(), request.worktree.git_dir())),
            CommandAccess::Write,
            Operation::RestoreSnapshot,
            &strings(&["clean", "-fdx", "--", "."]),
            None,
        )?;
        let restored_tree = self.write_tree(request.worktree, Operation::RestoreSnapshot)?;
        if restored_tree != request.snapshot.tree() {
            return Err(GitError::new(
                ErrorKind::Indeterminate,
                Operation::RestoreSnapshot,
                RecoveryClass::Reconcile,
                "restored index tree does not match the requested snapshot",
            ));
        }
        let status = self.status(request.worktree)?;
        if status.head() != request.expected_head
            || !status.is_detached()
            || status.index_tree() != Some(restored_tree)
        {
            return Err(object_mismatch(
                Operation::RestoreSnapshot,
                "post-restore observation changed unexpectedly",
            ));
        }
        Ok(RestoreObservation { prior_tree, restored_tree, status })
    }

    /// Deletes only the exact retained reference if it still denotes this snapshot commit.
    ///
    /// # Errors
    ///
    /// Rejects repository or reference drift. Snapshot objects remain subject to normal Git GC.
    pub fn release_snapshot(&self, snapshot: &CandidateSnapshot) -> Result<(), GitError> {
        if snapshot.manifest.repository_digest() != self.identity.digest() {
            return Err(object_mismatch(
                Operation::ReleaseSnapshot,
                "snapshot belongs to another repository",
            ));
        }
        verify_retained(self, snapshot, Operation::ReleaseSnapshot)?;
        let arguments = vec![
            OsString::from("update-ref"),
            OsString::from("-d"),
            OsString::from(snapshot.reference().as_str()),
            OsString::from(snapshot.commit().to_string()),
        ];
        self.checked_repo_command(
            Operation::ReleaseSnapshot,
            CommandAccess::Write,
            &arguments,
            None,
        )?;
        Ok(())
    }

    pub(super) fn write_tree(
        &self,
        worktree: &RegisteredWorktree,
        operation: Operation,
    ) -> Result<TreeId, GitError> {
        let output = self.runner.checked(
            worktree.root(),
            Some(Self::worktree_location(worktree.root(), worktree.git_dir())),
            CommandAccess::Read,
            operation,
            &strings(&["write-tree"]),
            None,
        )?;
        Ok(TreeId::checked(ObjectId::parse(
            self.identity.object_format(),
            one_line(&output.stdout, operation)?,
            operation,
        )?))
    }

    fn verify_commit(&self, commit: CommitId, operation: Operation) -> Result<(), GitError> {
        let expression = format!("{commit}^{{commit}}");
        self.checked_repo_command(
            operation,
            CommandAccess::Read,
            &[OsString::from("cat-file"), OsString::from("-e"), OsString::from(expression)],
            None,
        )?;
        Ok(())
    }

    fn snapshot_parent(&self, commit: CommitId) -> Result<CommitId, GitError> {
        let expression = format!("{commit}^");
        let mut arguments = strings(&["rev-parse", "--verify", "--end-of-options"]);
        arguments.push(OsString::from(expression));
        let output = self.checked_repo_command(
            Operation::ReopenSnapshot,
            CommandAccess::Read,
            &arguments,
            None,
        )?;
        Ok(CommitId::checked(ObjectId::parse(
            self.identity.object_format(),
            one_line(&output.stdout, Operation::ReopenSnapshot)?,
            Operation::ReopenSnapshot,
        )?))
    }
}

fn require_writable(worktree: &RegisteredWorktree, operation: Operation) -> Result<(), GitError> {
    if worktree.access() == WorktreeAccess::Writable {
        Ok(())
    } else {
        Err(GitError::new(
            ErrorKind::InvalidInput,
            operation,
            RecoveryClass::CorrectRequest,
            "mutation is not permitted for a read-only worktree registration",
        ))
    }
}

fn snapshot_mismatch(detail: &'static str) -> GitError {
    GitError::new(
        ErrorKind::SnapshotConflict,
        Operation::ReopenSnapshot,
        RecoveryClass::Reconcile,
        detail,
    )
}
