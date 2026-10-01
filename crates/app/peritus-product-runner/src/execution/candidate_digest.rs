//! Exact candidate identity inspection for external handoff controls.

use std::path::Path;

use peritus_types::Sha256Digest;

use super::ProductRunner;
use crate::{ProductRunnerError, progress::WorkspaceCheckpoint};

impl ProductRunner {
    /// Fingerprints source bytes, kinds, and permissions independently of commit history.
    /// This permits retrying an explicit commit after some nested commits succeeded.
    ///
    /// # Errors
    /// Returns an error if the complete source snapshot cannot be read.
    pub fn candidate_source_digest(
        workspace_root: &Path,
    ) -> Result<Sha256Digest, ProductRunnerError> {
        crate::candidate::managed::ManagedBaseline::source_digest(workspace_root)
    }

    /// Computes the exact current candidate digest used by checkpoint identity.
    ///
    /// # Errors
    ///
    /// Returns a repository error when the managed worktree cannot be inspected completely.
    pub fn candidate_digest(workspace_root: &Path) -> Result<Sha256Digest, ProductRunnerError> {
        WorkspaceCheckpoint::capture(workspace_root).map(|checkpoint| checkpoint.digest())
    }

    /// Reconciles a durable checkpoint at a new process boundary. Execution-dependent evidence
    /// becomes stale because the new process has not yet observed an execution context; content,
    /// repository, and requirements evidence keeps its original dependency-based status.
    ///
    /// # Errors
    /// Returns an invariant error if the checkpoint sequence cannot advance or reconciliation
    /// detects invalid retained evidence.
    pub fn reconcile_checkpoint_after_restart(
        checkpoint: peritus_run_settlement::CandidateCheckpoint,
    ) -> Result<peritus_run_settlement::CandidateCheckpoint, ProductRunnerError> {
        let previous = checkpoint.identity();
        if previous.execution_digest().is_none() {
            return Ok(checkpoint);
        }
        let sequence = previous.checkpoint_sequence().checked_add(1).ok_or_else(|| {
            ProductRunnerError::new(
                crate::ProductRunnerErrorKind::InternalInvariant,
                "reconcile candidate after restart",
                "candidate checkpoint sequence overflowed",
            )
        })?;
        let identity = peritus_run_settlement::CandidateIdentity::new(
            previous.run_id(),
            previous.workspace_id(),
            previous.content_digest(),
            previous.repository_digest(),
            None,
            previous.requirements_revision(),
            sequence,
        )
        .map_err(restart_invariant)?;
        checkpoint.reobserve(identity).map_err(restart_invariant)
    }
}

fn restart_invariant(error: impl std::fmt::Display) -> ProductRunnerError {
    ProductRunnerError::new(
        crate::ProductRunnerErrorKind::InternalInvariant,
        "reconcile candidate after restart",
        error.to_string(),
    )
}

impl ProductRunner {
    /// Reads the task baseline for embedding in its durable deliverable record.
    ///
    /// # Errors
    /// Rejects unreadable or invalid retained preimages.
    pub fn retained_task_baseline(trace: &Path) -> Result<String, ProductRunnerError> {
        let baseline =
            crate::candidate::managed::ManagedBaseline::load(&trace.with_extension("baseline"))?
                .ok_or_else(|| baseline_error("task baseline sidecar is missing"))?;
        serde_json::to_string(&baseline).map_err(baseline_error)
    }

    /// Validates an embedded task baseline without workspace effects.
    ///
    /// # Errors
    /// Rejects malformed paths, index stages, modes, or object identities.
    pub fn validate_task_baseline(bytes: &str) -> Result<(), ProductRunnerError> {
        parse_baseline(bytes).map(|_| ())
    }

    /// Exports task-owned changes using the baseline embedded in the durable run record.
    ///
    /// # Errors
    /// Rejects invalid preimages or unreadable workspace state.
    pub fn candidate_patch_from_baseline(
        workspace: &Path,
        bytes: &str,
    ) -> Result<Vec<u8>, ProductRunnerError> {
        parse_baseline(bytes)?.patch(workspace)
    }

    /// Discards task-owned changes using durable preimages even if the sidecar was lost.
    /// Returns directories preserving new repositories or recovery records for restored HEADs.
    ///
    /// # Errors
    /// Rejects unsafe or incomplete restoration before destructive preparation where possible.
    pub fn discard_from_baseline(
        workspace: &Path,
        bytes: &str,
        paths: &[String],
    ) -> Result<Vec<std::path::PathBuf>, ProductRunnerError> {
        let paths = paths.iter().map(std::path::PathBuf::from).collect::<Vec<_>>();
        parse_baseline(bytes)?.discard(workspace, &paths)
    }

    /// Retains exact discard preimages and ownership before restoration can start.
    /// The caller must retain the returned digest in its original command receipt.
    ///
    /// # Errors
    /// Rejects changed candidates, foreign intents, invalid preimages, or failed persistence.
    pub fn prepare_discard_transaction(
        workspace: &Path,
        baseline: &str,
        paths: &[String],
        state: &Path,
        binding: Sha256Digest,
        candidate: Sha256Digest,
    ) -> Result<Sha256Digest, ProductRunnerError> {
        let mut paths = paths.iter().map(std::path::PathBuf::from).collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        crate::candidate::managed::transaction::prepare(
            workspace,
            parse_baseline(baseline)?,
            paths,
            state,
            binding.into_bytes(),
            candidate.into_bytes(),
        )
        .map(Sha256Digest::new)
    }

    /// Reads bound discard progress without performing or retrying any restoration.
    ///
    /// # Errors
    /// Rejects corrupt or foreign records instead of acknowledging their effects.
    pub fn inspect_discard_transaction(
        state: &Path,
        binding: Sha256Digest,
        digest: Sha256Digest,
    ) -> Result<Option<crate::DiscardTransactionState>, ProductRunnerError> {
        crate::candidate::managed::transaction::inspect(
            state,
            binding.into_bytes(),
            digest.into_bytes(),
        )
    }

    /// Explicitly resumes the exact bound discard after fencing current owned values.
    /// A completed record returns its retained history without touching newer source.
    ///
    /// # Errors
    /// Preserves conflicting edits, foreign locks, and unrecognized recovery state.
    pub fn execute_discard_transaction(
        state: &Path,
        binding: Sha256Digest,
        digest: Sha256Digest,
    ) -> Result<Vec<std::path::PathBuf>, ProductRunnerError> {
        crate::candidate::managed::transaction::execute(
            state,
            binding.into_bytes(),
            digest.into_bytes(),
        )
    }
}

fn parse_baseline(
    bytes: &str,
) -> Result<crate::candidate::managed::ManagedBaseline, ProductRunnerError> {
    let baseline: crate::candidate::managed::ManagedBaseline =
        serde_json::from_str(bytes).map_err(baseline_error)?;
    baseline.validate()?;
    Ok(baseline)
}
fn baseline_error(error: impl std::fmt::Display) -> ProductRunnerError {
    ProductRunnerError::new(
        crate::ProductRunnerErrorKind::Repository,
        "retain task baseline",
        error.to_string(),
    )
}
