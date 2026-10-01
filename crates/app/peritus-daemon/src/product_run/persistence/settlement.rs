//! Stable JSON projection of verified candidate checkpoints and settlements.

use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceDependencies, EvidenceRecord,
    EvidenceStatus, QualificationEvidence, RunSettlement, SettlementCause, SettlementReducer,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use serde::Deserialize;
use serde::Serialize;

use crate::product_run::ProductRunServiceError;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedCheckpoint {
    identity: PersistedIdentity,
    stage: u16,
    gates: PersistedEvidence,
    obligations: PersistedEvidence,
    review: PersistedEvidence,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedIdentity {
    run_id: [u8; 16],
    workspace_id: [u8; 16],
    content_digest: [u8; 32],
    repository_digest: [u8; 32],
    execution_digest: Option<[u8; 32]>,
    requirements_revision: u64,
    checkpoint_sequence: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedEvidence {
    status: u16,
    provenance: Option<PersistedIdentity>,
    dependencies: Option<u16>,
    value: Option<u16>,
}

impl PersistedCheckpoint {
    pub(super) fn from_checkpoint(value: &CandidateCheckpoint) -> Self {
        Self {
            identity: PersistedIdentity::from_identity(*value.identity()),
            stage: value.stage().tag(),
            gates: PersistedEvidence::from_evidence(value.gates()),
            obligations: PersistedEvidence::from_evidence(value.obligations()),
            review: PersistedEvidence::from_evidence(value.review()),
        }
    }

    pub(super) fn into_checkpoint(self) -> Result<CandidateCheckpoint, ProductRunServiceError> {
        let identity = self.identity.into_identity()?;
        let stage =
            CandidateStage::from_tag(self.stage).ok_or(ProductRunServiceError::InvalidMessage)?;
        CandidateCheckpoint::new(
            identity,
            stage,
            self.gates.into_evidence()?,
            self.obligations.into_evidence()?,
            self.review.into_evidence()?,
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)
    }
}

impl PersistedIdentity {
    fn from_identity(value: CandidateIdentity) -> Self {
        Self {
            run_id: *value.run_id().as_bytes(),
            workspace_id: *value.workspace_id().as_bytes(),
            content_digest: value.content_digest().into_bytes(),
            repository_digest: value.repository_digest().into_bytes(),
            execution_digest: value.execution_digest().map(Sha256Digest::into_bytes),
            requirements_revision: value.requirements_revision(),
            checkpoint_sequence: value.checkpoint_sequence(),
        }
    }

    fn into_identity(self) -> Result<CandidateIdentity, ProductRunServiceError> {
        CandidateIdentity::new(
            RunId::new(self.run_id).map_err(|_| ProductRunServiceError::InvalidMessage)?,
            WorkspaceId::new(self.workspace_id)
                .map_err(|_| ProductRunServiceError::InvalidMessage)?,
            Sha256Digest::new(self.content_digest),
            Sha256Digest::new(self.repository_digest),
            self.execution_digest.map(Sha256Digest::new),
            self.requirements_revision,
            self.checkpoint_sequence,
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)
    }
}

impl PersistedEvidence {
    fn from_evidence(value: &EvidenceStatus<QualificationEvidence>) -> Self {
        Self {
            status: value.tag(),
            provenance: value
                .record()
                .map(|record| PersistedIdentity::from_identity(*record.provenance())),
            dependencies: value.record().map(|record| record.dependencies().tag()),
            value: value.record().map(|record| record.value().tag()),
        }
    }

    fn into_evidence(
        self,
    ) -> Result<EvidenceStatus<QualificationEvidence>, ProductRunServiceError> {
        if self.status == 1 {
            if self.provenance.is_some() || self.dependencies.is_some() || self.value.is_some() {
                return Err(ProductRunServiceError::InvalidMessage);
            }
            return Ok(EvidenceStatus::Missing);
        }
        let provenance =
            self.provenance.ok_or(ProductRunServiceError::InvalidMessage)?.into_identity()?;
        let dependencies = EvidenceDependencies::from_tag(
            self.dependencies.ok_or(ProductRunServiceError::InvalidMessage)?,
        )
        .ok_or(ProductRunServiceError::InvalidMessage)?;
        let value = QualificationEvidence::from_tag(
            self.value.ok_or(ProductRunServiceError::InvalidMessage)?,
        )
        .ok_or(ProductRunServiceError::InvalidMessage)?;
        let record = EvidenceRecord::new(provenance, dependencies, value);
        match self.status {
            2 => Ok(EvidenceStatus::Current(record)),
            3 => Ok(EvidenceStatus::Failed(record)),
            4 => Ok(EvidenceStatus::Stale(record)),
            _ => Err(ProductRunServiceError::InvalidMessage),
        }
    }
}

pub(super) fn restore_settlement(
    checkpoint: Option<CandidateCheckpoint>,
    cause_tag: Option<u16>,
) -> Result<Option<RunSettlement>, ProductRunServiceError> {
    let Some(tag) = cause_tag else { return Ok(None) };
    let cause = SettlementCause::from_tag(tag).ok_or(ProductRunServiceError::InvalidMessage)?;
    let mut reducer = SettlementReducer::new();
    if let Some(checkpoint) = checkpoint {
        reducer.observe(checkpoint).map_err(|_| ProductRunServiceError::InvalidMessage)?;
    }
    reducer.settle(cause).map(Some).map_err(|_| ProductRunServiceError::InvalidMessage)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dependency_aware_checkpoint_round_trips_exactly() {
        let checkpoint = dependency_checkpoint();
        let encoded = serde_json::to_vec(&PersistedCheckpoint::from_checkpoint(&checkpoint))
            .expect("encode checkpoint");
        let persisted: PersistedCheckpoint =
            serde_json::from_slice(&encoded).expect("decode checkpoint");

        assert_eq!(persisted.into_checkpoint().expect("restore checkpoint"), checkpoint);
    }

    fn dependency_checkpoint() -> CandidateCheckpoint {
        let identity = CandidateIdentity::new(
            RunId::new([1; 16]).expect("run"),
            WorkspaceId::new([2; 16]).expect("workspace"),
            Sha256Digest::new([3; 32]),
            Sha256Digest::new([4; 32]),
            Some(Sha256Digest::new([5; 32])),
            6,
            7,
        )
        .expect("identity");
        let evidence = |dependencies| {
            EvidenceStatus::Current(EvidenceRecord::new(
                identity,
                dependencies,
                QualificationEvidence::Satisfied,
            ))
        };
        CandidateCheckpoint::new(
            identity,
            CandidateStage::Qualified,
            evidence(EvidenceDependencies::GATES),
            evidence(EvidenceDependencies::OBLIGATIONS),
            evidence(EvidenceDependencies::REVIEW),
        )
        .expect("checkpoint")
    }
}
