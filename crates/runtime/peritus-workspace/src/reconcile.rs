//! Restart and post-fence workspace reconciliation.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore, FinalizedArtifact};
use peritus_git::{ReconcileDisposition as GitDisposition, ReconcileExpectation};
use peritus_leases::{ReconciliationCorrelation, ReconciliationDisposition};
use peritus_patch::{RecoveryBinding, RecoveryState};
use peritus_types::{EventId, EvidenceId, Sha256Digest};

use crate::verified::reconciliation_is_safe;
use crate::{WorkspaceCondition, WorkspaceError, WorkspaceGateway, WorkspaceManifest};

/// Exact facts gathered by the transaction and Git inspectors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconciliationInput {
    expected: ReconciliationCorrelation,
    observed: ReconciliationCorrelation,
    inspection_complete: bool,
    transaction_clean: bool,
    git_clean: bool,
    detail_digest: Sha256Digest,
}

impl ReconciliationInput {
    /// Creates an unprivileged inspection projection for deterministic classification.
    #[must_use]
    pub const fn new(
        expected: ReconciliationCorrelation,
        observed: ReconciliationCorrelation,
        inspection_complete: bool,
        transaction_clean: bool,
        git_clean: bool,
        detail_digest: Sha256Digest,
    ) -> Self {
        Self {
            expected,
            observed,
            inspection_complete,
            transaction_clean,
            git_clean,
            detail_digest,
        }
    }
}

/// Closed restart classification. Only a complete exact observation can be clean.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RestartDisposition {
    /// Exact correlation, transaction, Git, and filesystem state are clean.
    Clean,
    /// Complete inspection found a known divergence.
    Dirty,
    /// The exact observed workspace scope, generation, or prior holder differs.
    Fenced,
    /// Inspection or correlation was incomplete or ambiguous.
    Indeterminate,
}

/// Evidence fields over which the classification digest was computed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconciliationEvidence {
    correlation: ReconciliationCorrelation,
    disposition: RestartDisposition,
    detail_digest: Sha256Digest,
    digest: Sha256Digest,
}

impl ReconciliationEvidence {
    /// Returns the exact post-fence correlation.
    #[must_use]
    pub const fn correlation(self) -> ReconciliationCorrelation {
        self.correlation
    }
    /// Returns the classified restart state.
    #[must_use]
    pub const fn disposition(self) -> RestartDisposition {
        self.disposition
    }
    /// Returns the complete inspector-detail digest.
    #[must_use]
    pub const fn detail_digest(self) -> Sha256Digest {
        self.detail_digest
    }
    /// Returns the canonical evidence digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
}

/// Exact classifier result suitable for later C2/E0 correlation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RestartObservation {
    evidence: ReconciliationEvidence,
}

/// Finalized C0 record of one target-owned restart inspection.
pub struct ReconciliationOutcome {
    observation: RestartObservation,
    manifest: WorkspaceManifest,
    artifact: FinalizedArtifact,
}

impl ReconciliationOutcome {
    /// Returns the closed inspection result.
    #[must_use]
    pub const fn observation(&self) -> RestartObservation {
        self.observation
    }
    /// Borrows the canonical non-mutation reconciliation manifest.
    #[must_use]
    pub const fn manifest(&self) -> &WorkspaceManifest {
        &self.manifest
    }
    /// Returns the finalized C0 artifact identity.
    #[must_use]
    pub const fn artifact_digest(&self) -> ArtifactDigest {
        self.artifact.digest()
    }
}

impl RestartObservation {
    /// Returns the closed disposition.
    #[must_use]
    pub const fn disposition(self) -> RestartDisposition {
        self.evidence.disposition
    }
    /// Returns the exact evidence projection.
    #[must_use]
    pub const fn evidence(self) -> ReconciliationEvidence {
        self.evidence
    }

    /// Converts only dirty/indeterminate resource evidence to B1. C1 deliberately cannot construct
    /// `SafeToAcquire`, because C2 must independently supply holder-quiescence evidence.
    #[must_use]
    pub const fn unsafe_lease_disposition(
        self,
        evidence_id: EvidenceId,
    ) -> Option<ReconciliationDisposition> {
        match self.disposition() {
            RestartDisposition::Clean => None,
            RestartDisposition::Dirty => Some(ReconciliationDisposition::Dirty { evidence_id }),
            RestartDisposition::Fenced | RestartDisposition::Indeterminate => {
                Some(ReconciliationDisposition::Indeterminate { evidence_id })
            }
        }
    }
}

/// Applies the verified restart classifier without guessing from missing information.
#[must_use]
pub fn classify(input: ReconciliationInput) -> RestartObservation {
    let correlation_exact = input.expected == input.observed;
    let disposition = if !correlation_exact {
        RestartDisposition::Fenced
    } else if reconciliation_is_safe(
        correlation_exact,
        input.inspection_complete,
        input.transaction_clean,
        input.git_clean,
    ) {
        RestartDisposition::Clean
    } else if correlation_exact && input.inspection_complete {
        RestartDisposition::Dirty
    } else {
        RestartDisposition::Indeterminate
    };
    let digest = evidence_digest(input.observed, disposition, input.detail_digest);
    RestartObservation {
        evidence: ReconciliationEvidence {
            correlation: input.observed,
            disposition,
            detail_digest: input.detail_digest,
            digest,
        },
    }
}

impl WorkspaceGateway {
    /// Inspects every restart-visible patch transaction plus exact Git/index/filesystem state and
    /// applies the fail-closed classifier for one fenced correlation.
    ///
    /// # Errors
    ///
    /// Environmental inspection failures are represented as `Indeterminate`, not returned as a
    /// guessed clean state. The result is returned only after its canonical reconciliation
    /// manifest is durably finalized in the artifact store.
    #[allow(clippy::missing_const_for_fn, reason = "performs filesystem and Git observations")]
    pub fn reconcile_restart(
        &mut self,
        expected: ReconciliationCorrelation,
        artifacts: &ArtifactStore,
        creating_event: EventId,
    ) -> Result<ReconciliationOutcome, WorkspaceError> {
        let state = self.state();
        let observed = ReconciliationCorrelation::new(
            peritus_leases::LeaseScope::new(
                state.binding().workspace_id(),
                state.binding().resource_id(),
                state.binding().environment_id(),
            ),
            state.generation(),
            state.lease_holder(),
        );
        let (transaction_complete, transaction_clean, transaction_digest) =
            inspect_transactions(self.workspace_mut());
        let repository = self.workspace_mut().repository().clone();
        let worktree = self.workspace_mut().worktree().clone();
        let git = repository.reconcile(ReconcileExpectation::new(
            &worktree,
            self.state().binding().baseline_commit(),
            self.state().current_snapshot().tree(),
        ));
        let (git_complete, git_clean, git_digest) = git.map_or_else(
            |_| (false, false, Sha256Digest::new([0; 32])),
            |observation| {
                (
                    !matches!(observation.disposition(), GitDisposition::Indeterminate(_)),
                    matches!(observation.disposition(), GitDisposition::Clean),
                    observation.evidence_digest(),
                )
            },
        );
        let detail_digest = combine_inspection_digests(transaction_digest, git_digest);
        let observation = classify(ReconciliationInput::new(
            expected,
            observed,
            transaction_complete && git_complete,
            transaction_clean,
            git_clean,
            detail_digest,
        ));
        let condition = match observation.disposition() {
            RestartDisposition::Clean => WorkspaceCondition::Clean,
            RestartDisposition::Dirty => WorkspaceCondition::Dirty,
            RestartDisposition::Fenced => WorkspaceCondition::Reconciling,
            RestartDisposition::Indeterminate => WorkspaceCondition::Indeterminate,
        };
        self.workspace_mut().state_mut().set_condition(condition);
        let state = self.state();
        let manifest = WorkspaceManifest::reconciliation(
            state.binding().workspace_id(),
            state.generation(),
            state.revision(),
            state.current_snapshot().tree(),
            observation,
        );
        let artifact = manifest.finalize(artifacts, creating_event).inspect_err(|_| {
            self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
        })?;
        Ok(ReconciliationOutcome { observation, manifest, artifact })
    }
}

fn inspect_transactions(workspace: &crate::WritableWorkspace) -> (bool, bool, Sha256Digest) {
    let Ok(entries) = std::fs::read_dir(workspace.transaction_root()) else {
        return (false, false, Sha256Digest::new([0; 32]));
    };
    let mut paths = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            return (false, false, Sha256Digest::new([0; 32]));
        };
        paths.push(entry.path());
    }
    paths.sort();
    let mut complete = true;
    let mut clean = true;
    let mut bytes = b"PERITUS-WORKSPACE-TRANSACTION-INSPECTION-V1\0".to_vec();
    let root = workspace.root().to_owned();
    let state = workspace.state();
    let expected_binding =
        RecoveryBinding::new(state.binding().workspace_id(), state.generation(), state.revision());
    if crate::transaction_namespace::binding_is_exact(workspace.transaction_root(), state) {
        bytes.push(1);
    } else {
        complete = false;
        clean = false;
        bytes.push(0);
    }
    for path in paths {
        observe_entry_name(&mut bytes, &path);
        if path == crate::transaction_namespace::binding_manifest_path(workspace.transaction_root())
        {
            bytes.push(2);
            continue;
        }
        if path == crate::consumption::action_ledger_root(workspace.transaction_root()) {
            bytes.push(3);
            continue;
        }
        if !crate::transaction_namespace::is_canonical_transaction_directory(&path) {
            clean = false;
            bytes.push(4);
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            complete = false;
            clean = false;
            bytes.push(5);
            continue;
        };
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            clean = false;
            bytes.push(6);
            continue;
        }
        bytes.push(7);
        if let Ok(outcome) = peritus_patch::recover_transaction(&root, &path, expected_binding) {
            bytes.push(match outcome.state() {
                RecoveryState::RolledBackCleanly => 1,
                RecoveryState::AlreadyApplied => {
                    clean = false;
                    2
                }
                RecoveryState::Dirty => {
                    clean = false;
                    3
                }
                RecoveryState::Indeterminate => {
                    complete = false;
                    clean = false;
                    4
                }
            });
            if let Some(identity) = outcome.identity() {
                bytes.extend_from_slice(identity.as_bytes());
            }
            if let Some(binding) = outcome.binding() {
                bytes.extend_from_slice(binding.workspace_id().as_bytes());
                bytes.extend_from_slice(&binding.generation().get().to_be_bytes());
                bytes.extend_from_slice(&binding.revision().get().to_be_bytes());
                if binding != expected_binding {
                    complete = false;
                    clean = false;
                }
            } else {
                complete = false;
                clean = false;
            }
            bytes.push(u8::from(outcome.quarantined()));
            bytes.push(u8::from(outcome.cleanup_pending()));
        } else {
            complete = false;
            clean = false;
            bytes.push(5);
        }
    }
    (complete, clean, peritus_codec::sha256(&bytes))
}

fn observe_entry_name(bytes: &mut Vec<u8>, path: &std::path::Path) {
    let name = path.file_name().map_or(&[][..], std::ffi::OsStr::as_encoded_bytes);
    bytes.extend_from_slice(&(name.len() as u64).to_be_bytes());
    bytes.extend_from_slice(name);
}

fn combine_inspection_digests(left: Sha256Digest, right: Sha256Digest) -> Sha256Digest {
    let mut bytes = b"PERITUS-WORKSPACE-RESTART-DETAIL-V1\0".to_vec();
    bytes.extend_from_slice(left.as_bytes());
    bytes.extend_from_slice(right.as_bytes());
    peritus_codec::sha256(&bytes)
}

fn evidence_digest(
    correlation: ReconciliationCorrelation,
    disposition: RestartDisposition,
    detail: Sha256Digest,
) -> Sha256Digest {
    let scope = correlation.scope();
    let holder = correlation.prior_holder();
    let mut bytes = b"PERITUS-WORKSPACE-RECONCILIATION-V1\0".to_vec();
    bytes.extend_from_slice(scope.workspace_id().as_bytes());
    bytes.extend_from_slice(scope.resource_id().as_bytes());
    bytes.extend_from_slice(scope.environment_id().as_bytes());
    bytes.extend_from_slice(&correlation.fenced_generation().get().to_be_bytes());
    bytes.extend_from_slice(holder.actor_id().as_bytes());
    bytes.extend_from_slice(holder.session_id().as_bytes());
    bytes.push(match disposition {
        RestartDisposition::Clean => 1,
        RestartDisposition::Dirty => 2,
        RestartDisposition::Fenced => 3,
        RestartDisposition::Indeterminate => 4,
    });
    bytes.extend_from_slice(detail.as_bytes());
    peritus_codec::sha256(&bytes)
}

#[cfg(test)]
#[path = "reconcile_tests.rs"]
mod tests;
