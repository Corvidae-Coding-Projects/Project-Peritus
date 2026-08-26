//! Complete canonical dataset validation and digest binding.

use std::collections::BTreeSet;

use peritus_codec::{CanonicalWriter, CodecLimits};

use crate::{
    DatasetDigest, DatasetId, DatasetTask, EvaluationError, EvaluationErrorKind, EvaluationLimits,
    EvaluationOperation,
};

const DATASET_DOMAIN: &[u8] = b"peritus.evaluation.dataset.v1\0";

/// Complete checked immutable dataset revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatasetManifest {
    id: DatasetId,
    revision: u64,
    tasks: Vec<DatasetTask>,
    provenance_digest: peritus_types::Sha256Digest,
    digest: DatasetDigest,
}

impl DatasetManifest {
    /// Validates and freezes a canonical dataset manifest.
    ///
    /// # Errors
    /// Rejects zero revision, empty/oversized tasks, noncanonical IDs, and any candidate/evaluator
    /// artifact collision across the complete corpus.
    pub fn new(
        id: DatasetId,
        revision: u64,
        tasks: Vec<DatasetTask>,
        provenance_digest: peritus_types::Sha256Digest,
        limits: EvaluationLimits,
    ) -> Result<Self, EvaluationError> {
        if revision == 0
            || tasks.is_empty()
            || tasks.len() > usize::try_from(limits.tasks()).unwrap_or(usize::MAX)
            || tasks.windows(2).any(|pair| pair[0].id() >= pair[1].id())
        {
            return Err(crate::invalid(
                EvaluationErrorKind::Manifest,
                EvaluationOperation::ValidateDataset,
                "dataset revision/tasks are empty, oversized, duplicated, or noncanonical",
            ));
        }
        let candidate: BTreeSet<_> =
            tasks.iter().map(|task| task.candidate_input().artifact()).collect();
        let evaluator: BTreeSet<_> =
            tasks.iter().map(|task| task.evaluator_input().artifact()).collect();
        if !candidate.is_disjoint(&evaluator) {
            return Err(crate::invalid(
                EvaluationErrorKind::Isolation,
                EvaluationOperation::ValidateDataset,
                "candidate and evaluator artifact sets overlap",
            ));
        }
        let digest = DatasetDigest::new(peritus_codec::sha256(&canonical_bytes(
            id,
            revision,
            &tasks,
            provenance_digest,
        )?));
        Ok(Self { id, revision, tasks, provenance_digest, digest })
    }

    /// Returns stable dataset lineage identity.
    #[must_use]
    pub const fn id(&self) -> DatasetId {
        self.id
    }
    /// Returns nonzero dataset revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Borrows canonical task descriptors.
    #[must_use]
    pub fn tasks(&self) -> &[DatasetTask] {
        &self.tasks
    }
    /// Returns exact provenance digest.
    #[must_use]
    pub const fn provenance_digest(&self) -> peritus_types::Sha256Digest {
        self.provenance_digest
    }
    /// Returns complete manifest digest.
    #[must_use]
    pub const fn digest(&self) -> DatasetDigest {
        self.digest
    }

    /// Returns exact canonical manifest bytes.
    ///
    /// # Errors
    /// Returns a bound error only if this checked value no longer fits production codec limits.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EvaluationError> {
        canonical_bytes(self.id, self.revision, &self.tasks, self.provenance_digest)
    }
}

fn canonical_bytes(
    id: DatasetId,
    revision: u64,
    tasks: &[DatasetTask],
    provenance: peritus_types::Sha256Digest,
) -> Result<Vec<u8>, EvaluationError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_bytes(DATASET_DOMAIN).map_err(codec)?;
    writer.write_fixed(id.as_bytes()).map_err(codec)?;
    writer.write_u64(revision).map_err(codec)?;
    writer.write_fixed(provenance.as_bytes()).map_err(codec)?;
    writer.write_collection_len(tasks.len()).map_err(codec)?;
    for task in tasks {
        writer.write_fixed(task.id().as_bytes()).map_err(codec)?;
        writer.write_u8(task.partition().tag()).map_err(codec)?;
        writer.write_u32(task.weight()).map_err(codec)?;
        writer.write_fixed(task.candidate_input().artifact().as_bytes()).map_err(codec)?;
        writer.write_u64(task.candidate_input().byte_length()).map_err(codec)?;
        writer.write_fixed(task.evaluator_input().artifact().as_bytes()).map_err(codec)?;
        writer.write_u64(task.evaluator_input().byte_length()).map_err(codec)?;
        writer.write_fixed(task.evaluator_input().verifier_digest().as_bytes()).map_err(codec)?;
        writer.write_fixed(task.resource_profile_digest().as_bytes()).map_err(codec)?;
    }
    Ok(writer.into_bytes())
}

const fn codec(_: peritus_codec::CodecError) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::LimitExceeded,
        EvaluationOperation::ValidateDataset,
        crate::EvaluationRecovery::ReduceScope,
        "canonical dataset exceeds production codec limits",
    )
}
