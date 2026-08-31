//! Atomic containment for divergent retained snapshot references.

use std::ffi::OsString;

use crate::command::CommandAccess;
use crate::{ErrorKind, GitError, GitRepository, Operation, RecoveryClass};

use super::SnapshotQuarantine;
use super::support::{observe_ref, quarantine_ref, snapshot_ref};

impl GitRepository {
    /// Atomically moves a divergent retained snapshot reference out of the active namespace.
    ///
    /// The quarantine reference preserves the observed value for operator inspection. Repeating
    /// this operation after a successful move returns the same observation without changing Git.
    ///
    /// # Errors
    ///
    /// Rejects foreign or noncanonical manifests, healthy active references, missing divergent
    /// state, an occupied quarantine reference, and Git compare-and-swap failures.
    pub fn quarantine_snapshot(
        &self,
        manifest: &crate::CandidateSnapshotManifest,
    ) -> Result<SnapshotQuarantine, GitError> {
        if manifest.repository_digest() != self.identity.digest() {
            return Err(quarantine_error("snapshot belongs to another repository"));
        }
        let active = snapshot_ref(manifest.workspace_id(), manifest.snapshot_id());
        if &active != manifest.reference() {
            return Err(quarantine_error("snapshot manifest reference is not canonical"));
        }
        let quarantine = quarantine_ref(manifest.workspace_id(), manifest.snapshot_id());
        let active_value = observe_ref(self, &active, Operation::QuarantineSnapshot)?;
        let quarantine_value = observe_ref(self, &quarantine, Operation::QuarantineSnapshot)?;
        match (active_value, quarantine_value) {
            (None, Some(observed)) if observed != manifest.commit() => {
                return Ok(SnapshotQuarantine {
                    active_reference: active,
                    quarantine_reference: quarantine,
                    observed_commit: observed,
                });
            }
            (Some(observed), None) if observed != manifest.commit() => {
                let input = format!(
                    "create {} {}\ndelete {} {}\n",
                    quarantine.as_str(),
                    observed,
                    active.as_str(),
                    observed
                );
                self.checked_repo_command(
                    Operation::QuarantineSnapshot,
                    CommandAccess::Write,
                    &[OsString::from("update-ref"), OsString::from("--stdin")],
                    Some(input.as_bytes()),
                )?;
                if observe_ref(self, &active, Operation::QuarantineSnapshot)?.is_some()
                    || observe_ref(self, &quarantine, Operation::QuarantineSnapshot)?
                        != Some(observed)
                {
                    return Err(quarantine_error(
                        "snapshot quarantine did not publish exact contained state",
                    ));
                }
                return Ok(SnapshotQuarantine {
                    active_reference: active,
                    quarantine_reference: quarantine,
                    observed_commit: observed,
                });
            }
            (Some(observed), _) if observed == manifest.commit() => {
                return Err(GitError::new(
                    ErrorKind::InvalidInput,
                    Operation::QuarantineSnapshot,
                    RecoveryClass::CorrectRequest,
                    "healthy snapshot reference cannot be quarantined",
                ));
            }
            _ => {}
        }
        Err(quarantine_error("snapshot quarantine state is missing, occupied, or indeterminate"))
    }
}

fn quarantine_error(detail: &'static str) -> GitError {
    GitError::new(
        ErrorKind::SnapshotConflict,
        Operation::QuarantineSnapshot,
        RecoveryClass::Reconcile,
        detail,
    )
}
