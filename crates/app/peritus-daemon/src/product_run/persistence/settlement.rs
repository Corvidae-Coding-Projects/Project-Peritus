//! Stable JSON projection of verified candidate checkpoints and settlements.

use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceRecord, EvidenceStatus,
    QualificationEvidence, RunSettlement, SettlementCause, SettlementReducer,
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
    candidate_digest: [u8; 32],
    conversation_revision: u64,
    checkpoint_sequence: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedEvidence {
    status: u16,
    provenance: Option<PersistedIdentity>,
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
    const fn from_identity(value: CandidateIdentity) -> Self {
        Self {
            run_id: *value.run_id().as_bytes(),
            workspace_id: *value.workspace_id().as_bytes(),
            candidate_digest: *value.candidate_digest().as_bytes(),
            conversation_revision: value.conversation_revision(),
            checkpoint_sequence: value.checkpoint_sequence(),
        }
    }

    fn into_identity(self) -> Result<CandidateIdentity, ProductRunServiceError> {
        let run_id = RunId::new(self.run_id).map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let workspace_id = WorkspaceId::new(self.workspace_id)
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        CandidateIdentity::new(
            run_id,
            workspace_id,
            Sha256Digest::new(self.candidate_digest),
            self.conversation_revision,
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
            value: value.record().map(|record| record.value().tag()),
        }
    }

    fn into_evidence(
        self,
    ) -> Result<EvidenceStatus<QualificationEvidence>, ProductRunServiceError> {
        if self.status == 1 {
            if self.provenance.is_some() || self.value.is_some() {
                return Err(ProductRunServiceError::InvalidMessage);
            }
            return Ok(EvidenceStatus::Missing);
        }
        let provenance =
            self.provenance.ok_or(ProductRunServiceError::InvalidMessage)?.into_identity()?;
        let value = QualificationEvidence::from_tag(
            self.value.ok_or(ProductRunServiceError::InvalidMessage)?,
        )
        .ok_or(ProductRunServiceError::InvalidMessage)?;
        let record = EvidenceRecord::new(provenance, value);
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
    use peritus_run_settlement::RunDisposition;

    #[test]
    fn checkpoint_and_settlement_round_trip_without_losing_evidence() {
        let identity = CandidateIdentity::new(
            RunId::new([1; 16]).expect("run"),
            WorkspaceId::new([2; 16]).expect("workspace"),
            Sha256Digest::new([3; 32]),
            4,
            5,
        )
        .expect("identity");
        let passing = EvidenceStatus::Current(EvidenceRecord::new(
            identity,
            QualificationEvidence::Satisfied,
        ));
        let checkpoint = CandidateCheckpoint::new(
            identity,
            CandidateStage::ReviewPending,
            passing,
            passing,
            EvidenceStatus::Missing,
        )
        .expect("checkpoint");

        let encoded = serde_json::to_vec(&PersistedCheckpoint::from_checkpoint(&checkpoint))
            .expect("encode checkpoint");
        let persisted: PersistedCheckpoint =
            serde_json::from_slice(&encoded).expect("decode checkpoint");
        let restored = persisted.into_checkpoint().expect("restore checkpoint");
        let settlement = restore_settlement(Some(restored), Some(SettlementCause::Review.tag()))
            .expect("settlement")
            .expect("present settlement");

        assert_eq!(restored, checkpoint);
        assert_eq!(settlement.disposition(), RunDisposition::CandidateAvailable);
        assert_eq!(settlement.checkpoint(), Some(&checkpoint));
    }
}
