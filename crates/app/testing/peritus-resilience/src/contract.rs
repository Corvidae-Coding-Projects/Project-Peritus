//! Runtime-neutral executable contract for integrated resilience subjects.

use std::future::Future;
use std::pin::Pin;

use crate::QualificationText;
use crate::{
    CancellationToken, CleanupObservation, DisruptionObservation, EvidenceDigest,
    PreparationObservation, RecoveryObservation, ScenarioSpec, SubjectError, SubjectId,
};

/// Boxed runtime-neutral asynchronous qualification operation.
pub type QualificationFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Immutable identity of an implementation under qualification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubjectDescriptor {
    id: SubjectId,
    implementation: QualificationText,
    build_digest: EvidenceDigest,
}

impl SubjectDescriptor {
    /// Creates a release-candidate identity bound to an exact build digest.
    #[must_use]
    pub const fn new(
        id: SubjectId,
        implementation: QualificationText,
        build_digest: EvidenceDigest,
    ) -> Self {
        Self { id, implementation, build_digest }
    }

    /// Returns the stable implementation identifier.
    #[must_use]
    pub const fn id(&self) -> &SubjectId {
        &self.id
    }
    /// Returns the bounded implementation description.
    #[must_use]
    pub const fn implementation(&self) -> &QualificationText {
        &self.implementation
    }
    /// Returns the exact qualified build digest.
    #[must_use]
    pub const fn build_digest(&self) -> EvidenceDigest {
        self.build_digest
    }
}

/// One isolated integrated Peritus instance used by exactly one scenario.
pub trait ResilienceSubject: Send {
    /// Establishes a deterministic active baseline for `scenario`.
    fn prepare<'a>(
        &'a mut self,
        scenario: &'a ScenarioSpec,
    ) -> QualificationFuture<'a, Result<PreparationObservation, SubjectError>>;

    /// Arms and triggers the exact fault, then reports whether the boundary was reached.
    fn inject<'a>(
        &'a mut self,
        scenario: &'a ScenarioSpec,
    ) -> QualificationFuture<'a, Result<DisruptionObservation, SubjectError>>;

    /// Restarts/reconciles the subject and returns direct final observations.
    fn recover<'a>(
        &'a mut self,
        scenario: &'a ScenarioSpec,
    ) -> QualificationFuture<'a, Result<RecoveryObservation, SubjectError>>;
}

/// Creates fresh isolated subjects and consumes them through bounded cleanup.
pub trait ResilienceSubjectFactory<S>: Send + Sync
where
    S: ResilienceSubject,
{
    /// Returns immutable release-candidate metadata.
    fn descriptor(&self) -> &SubjectDescriptor;

    /// Creates a fresh subject for exactly one scenario.
    ///
    /// The subject must retain or propagate `cancellation` to all owned asynchronous work. It must
    /// also own synchronous RAII cleanup because dropping a pending runner cannot await cleanup.
    fn create<'a>(
        &'a self,
        scenario: &'a ScenarioSpec,
        cancellation: CancellationToken,
    ) -> QualificationFuture<'a, Result<S, SubjectError>>;

    /// Consumes a created subject and proves bounded owned-resource cleanup.
    fn cleanup<'a>(
        &'a self,
        scenario: &'a ScenarioSpec,
        subject: S,
    ) -> QualificationFuture<'a, Result<CleanupObservation, SubjectError>>;
}
