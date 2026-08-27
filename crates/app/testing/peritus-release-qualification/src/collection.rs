//! Fresh-subject collection for release gates and representative campaigns.

use std::collections::BTreeSet;

use serde::Serialize;

use peritus_release_artifacts::ReleaseBinding;

use crate::{EvidenceKind, QualificationError, SignedEvidenceRecord, SubjectId};

/// Request for one exact campaign on a never-used subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionRequest {
    binding: ReleaseBinding,
    kind: EvidenceKind,
}

impl CollectionRequest {
    const fn new(binding: ReleaseBinding, kind: EvidenceKind) -> Self {
        Self { binding, kind }
    }

    /// Returns the exact release binding.
    #[must_use]
    pub const fn binding(&self) -> &ReleaseBinding {
        &self.binding
    }

    /// Returns the required evidence category.
    #[must_use]
    pub const fn kind(&self) -> EvidenceKind {
        self.kind
    }
}

/// Cleanup facts observed after closing a disposable subject.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CleanupObservation {
    processes: u32,
    mounts: u32,
    worktrees: u32,
    temporary_paths: u32,
}

impl CleanupObservation {
    /// Creates exact residual-resource counts from a subject adapter.
    #[must_use]
    pub const fn new(
        remaining_processes: u32,
        remaining_mounts: u32,
        remaining_worktrees: u32,
        remaining_temporary_paths: u32,
    ) -> Self {
        Self {
            processes: remaining_processes,
            mounts: remaining_mounts,
            worktrees: remaining_worktrees,
            temporary_paths: remaining_temporary_paths,
        }
    }

    /// Returns whether no subject-owned resource remained after closure.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.processes == 0 && self.mounts == 0 && self.worktrees == 0 && self.temporary_paths == 0
    }
}

/// Adapter for one disposable qualification subject.
pub trait QualificationSubject: Sized {
    /// Returns the adapter-observed unique subject identity.
    fn subject_id(&self) -> &SubjectId;

    /// Collects and externally signs the requested evidence envelope.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when collection or signature admission fails.
    fn collect(
        &mut self,
        request: &CollectionRequest,
    ) -> Result<SignedEvidenceRecord, QualificationError>;

    /// Closes the subject and reports exact residual-resource counts.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when cleanup could not be completely observed.
    fn close(self) -> Result<CleanupObservation, QualificationError>;
}

/// Factory that must provision a never-used subject for each request.
pub trait FreshSubjectFactory {
    /// Concrete subject owned until cleanup.
    type Subject: QualificationSubject;

    /// Provisions a new subject for exactly one campaign.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when a fresh subject cannot be provisioned.
    fn create(&mut self, request: &CollectionRequest) -> Result<Self::Subject, QualificationError>;
}

/// Stable collection failure class.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CollectionFailure {
    /// No subject could be provisioned.
    Provisioning,
    /// A previously used identity was returned.
    SubjectReused,
    /// The subject returned another campaign or release binding.
    EvidenceSubstituted,
    /// Campaign collection failed.
    Collection,
    /// Cleanup could not be observed.
    CleanupUnobserved,
    /// Cleanup left subject-owned resources behind.
    CleanupIncomplete,
}

/// Outcome of one fresh-subject case.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CollectionOutcome {
    /// Signed evidence was admitted and cleanup was complete.
    Passed,
    /// One or more protocol, collection, or cleanup checks failed.
    Failed,
}

/// Deterministic result for one campaign and subject.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CollectionCase {
    kind: EvidenceKind,
    subject_id: Option<SubjectId>,
    record: Option<SignedEvidenceRecord>,
    cleanup: Option<CleanupObservation>,
    failures: Vec<CollectionFailure>,
    outcome: CollectionOutcome,
}

impl CollectionCase {
    /// Returns the requested evidence category.
    #[must_use]
    pub const fn kind(&self) -> EvidenceKind {
        self.kind
    }

    /// Returns the fresh subject identity when provisioning succeeded.
    #[must_use]
    pub const fn subject_id(&self) -> Option<&SubjectId> {
        self.subject_id.as_ref()
    }

    /// Returns verified evidence when collection admitted it.
    #[must_use]
    pub const fn record(&self) -> Option<&SignedEvidenceRecord> {
        self.record.as_ref()
    }

    /// Returns cleanup facts when cleanup was observed.
    #[must_use]
    pub const fn cleanup(&self) -> Option<CleanupObservation> {
        self.cleanup
    }

    /// Returns stable failures in discovery order.
    #[must_use]
    pub fn failures(&self) -> &[CollectionFailure] {
        &self.failures
    }

    /// Returns the derived outcome.
    #[must_use]
    pub const fn outcome(&self) -> CollectionOutcome {
        self.outcome
    }
}

/// Complete ordered fresh-subject campaign run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CollectionRun {
    binding: ReleaseBinding,
    cases: Vec<CollectionCase>,
}

impl CollectionRun {
    /// Returns the exact release binding.
    #[must_use]
    pub const fn binding(&self) -> &ReleaseBinding {
        &self.binding
    }

    /// Returns cases in closed catalog order.
    #[must_use]
    pub fn cases(&self) -> &[CollectionCase] {
        &self.cases
    }

    /// Returns whether every required campaign passed with complete cleanup.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        let required = EvidenceKind::fresh_subject_campaigns();
        self.cases.len() == required.len()
            && self.cases.iter().zip(required).all(|(case, kind)| {
                case.kind == kind
                    && case.outcome == CollectionOutcome::Passed
                    && case.cleanup.is_some_and(CleanupObservation::is_complete)
            })
    }

    /// Iterates verified records without copying evidence.
    pub fn records(&self) -> impl Iterator<Item = &SignedEvidenceRecord> {
        self.cases.iter().filter_map(CollectionCase::record)
    }
}

/// Runs the closed campaign catalog with one distinct subject per campaign.
#[derive(Clone, Debug)]
pub struct FreshSubjectRunner {
    binding: ReleaseBinding,
}

impl FreshSubjectRunner {
    /// Creates a runner for one immutable candidate binding.
    #[must_use]
    pub const fn new(binding: ReleaseBinding) -> Self {
        Self { binding }
    }

    /// Runs all required collection campaigns and always attempts subject cleanup.
    ///
    /// Adapter failures are retained as failed cases so absence can never become success.
    pub fn run<F: FreshSubjectFactory>(&self, factory: &mut F) -> CollectionRun {
        let mut used_subjects = BTreeSet::new();
        let mut cases = Vec::with_capacity(EvidenceKind::fresh_subject_campaigns().len());
        for kind in EvidenceKind::fresh_subject_campaigns() {
            let request = CollectionRequest::new(self.binding.clone(), kind);
            let Ok(mut subject) = factory.create(&request) else {
                cases.push(CollectionCase {
                    kind,
                    subject_id: None,
                    record: None,
                    cleanup: None,
                    failures: vec![CollectionFailure::Provisioning],
                    outcome: CollectionOutcome::Failed,
                });
                continue;
            };
            let subject_id = subject.subject_id().clone();
            let reused = !used_subjects.insert(subject_id.clone());
            let collected = subject.collect(&request);
            let cleanup = subject.close();
            let mut failures = Vec::new();
            if reused {
                failures.push(CollectionFailure::SubjectReused);
            }
            let record = match collected {
                Ok(record)
                    if record.binding() == &self.binding
                        && record.evidence_reference().kind() == kind =>
                {
                    Some(record)
                }
                Ok(_) => {
                    failures.push(CollectionFailure::EvidenceSubstituted);
                    None
                }
                Err(_) => {
                    failures.push(CollectionFailure::Collection);
                    None
                }
            };
            let cleanup = if let Ok(observation) = cleanup {
                if !observation.is_complete() {
                    failures.push(CollectionFailure::CleanupIncomplete);
                }
                Some(observation)
            } else {
                failures.push(CollectionFailure::CleanupUnobserved);
                None
            };
            let outcome = if failures.is_empty() {
                CollectionOutcome::Passed
            } else {
                CollectionOutcome::Failed
            };
            cases.push(CollectionCase {
                kind,
                subject_id: Some(subject_id),
                record,
                cleanup,
                failures,
                outcome,
            });
        }
        CollectionRun { binding: self.binding.clone(), cases }
    }
}
