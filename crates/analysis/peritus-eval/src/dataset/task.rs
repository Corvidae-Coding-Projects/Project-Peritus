//! Checked task descriptors with disjoint candidate and evaluator inputs.

use peritus_artifact_store::ArtifactDigest;
use peritus_types::Sha256Digest;

use crate::{DatasetPartition, EvaluationError, EvaluationErrorKind, EvaluationOperation, TaskId};

/// Candidate-visible immutable task input artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CandidateTaskInput {
    artifact: ArtifactDigest,
    byte_length: u64,
}

impl CandidateTaskInput {
    /// Creates a candidate-visible input binding.
    ///
    /// # Errors
    /// Rejects an empty artifact.
    pub const fn new(artifact: ArtifactDigest, byte_length: u64) -> Result<Self, EvaluationError> {
        if byte_length == 0 {
            return Err(crate::invalid(
                EvaluationErrorKind::Manifest,
                EvaluationOperation::ValidateDataset,
                "candidate task artifact is empty",
            ));
        }
        Ok(Self { artifact, byte_length })
    }

    /// Returns the content-addressed artifact.
    #[must_use]
    pub const fn artifact(self) -> ArtifactDigest {
        self.artifact
    }
    /// Returns exact artifact bytes.
    #[must_use]
    pub const fn byte_length(self) -> u64 {
        self.byte_length
    }
}

/// Evaluator-only immutable hidden input and verifier binding.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SealedEvaluatorInput {
    artifact: ArtifactDigest,
    byte_length: u64,
    verifier_digest: Sha256Digest,
}

impl SealedEvaluatorInput {
    /// Creates a sealed evaluator binding.
    ///
    /// # Errors
    /// Rejects an empty evaluator artifact.
    pub const fn new(
        artifact: ArtifactDigest,
        byte_length: u64,
        verifier_digest: Sha256Digest,
    ) -> Result<Self, EvaluationError> {
        if byte_length == 0 {
            return Err(crate::invalid(
                EvaluationErrorKind::Manifest,
                EvaluationOperation::ValidateDataset,
                "sealed evaluator artifact is empty",
            ));
        }
        Ok(Self { artifact, byte_length, verifier_digest })
    }

    /// Returns the evaluator-only artifact root.
    #[must_use]
    pub const fn artifact(self) -> ArtifactDigest {
        self.artifact
    }
    /// Returns exact artifact bytes.
    #[must_use]
    pub const fn byte_length(self) -> u64 {
        self.byte_length
    }
    /// Returns the exact verifier implementation/schema digest.
    #[must_use]
    pub const fn verifier_digest(self) -> Sha256Digest {
        self.verifier_digest
    }
}

/// One canonical immutable evaluation task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatasetTask {
    id: TaskId,
    partition: DatasetPartition,
    weight: u32,
    candidate: CandidateTaskInput,
    evaluator: SealedEvaluatorInput,
    resource_profile_digest: Sha256Digest,
}

impl DatasetTask {
    /// Creates a checked task with disjoint candidate and evaluator artifacts.
    ///
    /// # Errors
    /// Rejects zero weight or artifact aliasing across the isolation boundary.
    pub fn new(
        id: TaskId,
        partition: DatasetPartition,
        weight: u32,
        candidate: CandidateTaskInput,
        evaluator: SealedEvaluatorInput,
        resource_profile_digest: Sha256Digest,
    ) -> Result<Self, EvaluationError> {
        if weight == 0 {
            return Err(crate::invalid(
                EvaluationErrorKind::Manifest,
                EvaluationOperation::ValidateDataset,
                "dataset task weight is zero",
            ));
        }
        if candidate.artifact() == evaluator.artifact() {
            return Err(crate::invalid(
                EvaluationErrorKind::Isolation,
                EvaluationOperation::ValidateDataset,
                "candidate and evaluator artifacts are identical",
            ));
        }
        Ok(Self { id, partition, weight, candidate, evaluator, resource_profile_digest })
    }

    /// Returns task identity.
    #[must_use]
    pub const fn id(&self) -> TaskId {
        self.id
    }
    /// Returns the declared partition.
    #[must_use]
    pub const fn partition(&self) -> DatasetPartition {
        self.partition
    }
    /// Returns nonzero task weight.
    #[must_use]
    pub const fn weight(&self) -> u32 {
        self.weight
    }
    /// Returns candidate-visible input only.
    #[must_use]
    pub const fn candidate_input(&self) -> CandidateTaskInput {
        self.candidate
    }
    /// Returns evaluator-only input for a separately authorized directive.
    #[must_use]
    pub const fn evaluator_input(&self) -> SealedEvaluatorInput {
        self.evaluator
    }
    /// Returns the frozen resource-profile digest.
    #[must_use]
    pub const fn resource_profile_digest(&self) -> Sha256Digest {
        self.resource_profile_digest
    }
}
