//! Provenance-checked evidence admission for claimed F0 publication directives.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_evidence::{
    EvidenceDraft, EvidenceKind, EvidenceRecord, EvidenceSource, EvidenceStore,
};
use peritus_journal::SqliteJournal;
use peritus_types::EvidenceId;

use crate::{
    EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionPublicationClaim,
    EvolutionPublicationKind, EvolutionRecovery, FinalizedEvolutionArtifact,
};

/// Exact admitted evidence after its C0 directive was acknowledged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvolutionPublication {
    evidence: EvidenceRecord,
}

impl EvolutionPublication {
    /// Immutable provenance-checked evidence record.
    #[must_use]
    pub const fn evidence(&self) -> &EvidenceRecord {
        &self.evidence
    }
    /// Consumes the publication observation.
    #[must_use]
    pub fn into_evidence(self) -> EvidenceRecord {
        self.evidence
    }
}

/// Verifies, admits, and idempotently acknowledges one exact claimed F0 publication.
///
/// # Errors
/// Rejects claim/artifact/revision drift or any artifact, evidence, integrity, or outbox failure.
#[allow(clippy::too_many_arguments)]
pub fn publish_claimed_evolution(
    journal: &mut SqliteJournal,
    evidence_store: &mut EvidenceStore,
    artifact_store: &ArtifactStore,
    claim: &EvolutionPublicationClaim,
    artifact: FinalizedEvolutionArtifact,
) -> Result<EvolutionPublication, EvolutionError> {
    let directive = claim.directive();
    if artifact.artifact_digest().sha256() != directive.artifact_digest()
        || artifact.semantic_digest() != directive.evidence_digest()
        || claim.producing_position() == 0
    {
        return Err(binding("publication claim, artifact, or semantic digest differs"));
    }
    let evidence = if let Some(existing) =
        evidence_store.load(evidence_id(claim)?).map_err(evidence_error)?
    {
        let draft =
            publication_draft(claim, artifact.artifact_digest(), existing.artifacts().to_vec())?;
        evidence_store
            .committed_retry(&draft)
            .map_err(evidence_error)?
            .ok_or_else(|| binding("committed publication evidence disappeared during retry"))?
    } else {
        let export = journal.integrity_export().map_err(journal_error)?;
        let references = export.artifact_references();
        let start = references
            .partition_point(|reference| reference.last_position() < claim.producing_position());
        let end = references
            .partition_point(|reference| reference.first_position() <= claim.producing_position());
        let artifacts = references[start..end]
            .iter()
            .map(|reference| ArtifactDigest::from_sha256(reference.artifact_digest()))
            .collect();
        let draft = publication_draft(claim, artifact.artifact_digest(), artifacts)?;
        evidence_store.admit(draft, &export, artifact_store).map_err(evidence_error)?
    };
    journal.acknowledge_outbox(claim.id(), claim.fence()).map_err(journal_error)?;
    Ok(EvolutionPublication { evidence })
}

fn publication_draft(
    claim: &EvolutionPublicationClaim,
    artifact: ArtifactDigest,
    artifacts: Vec<ArtifactDigest>,
) -> Result<EvidenceDraft, EvolutionError> {
    if !artifacts.contains(&artifact) {
        return Err(binding("publication artifact is absent from the producing journal batch"));
    }
    let directive = claim.directive();
    let kind = match directive.kind() {
        EvolutionPublicationKind::CampaignDecision => "evolution-decision",
        EvolutionPublicationKind::HarnessActivation => "harness-activation",
    };
    EvidenceDraft::new(
        evidence_id(claim)?,
        EvidenceKind::new(kind).map_err(evidence_error)?,
        EvidenceSource::new("peritus-evolution").map_err(evidence_error)?,
        directive.revision(),
        claim.producing_position(),
        directive.evidence_digest(),
        artifacts,
        Vec::new(),
    )
    .map_err(evidence_error)
}

fn evidence_id(claim: &EvolutionPublicationClaim) -> Result<EvidenceId, EvolutionError> {
    let mut bytes = b"PERITUS-F0-EVIDENCE-ID\0".to_vec();
    bytes.extend_from_slice(claim.id().as_bytes());
    bytes.extend_from_slice(&claim.producing_position().to_be_bytes());
    let digest = peritus_codec::sha256(&bytes);
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    if id == [0; 16] {
        id[15] = 1;
    }
    EvidenceId::new(id).map_err(|_| binding("derived evidence identity is invalid"))
}

const fn binding(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::Publish,
        EvolutionRecovery::Quarantine,
        detail,
    )
}

fn evidence_error(_: impl core::fmt::Display) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Evidence,
        EvolutionOperation::Publish,
        EvolutionRecovery::Reconcile,
        "evolution evidence admission failed",
    )
}

fn journal_error(_: impl core::fmt::Display) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Journal,
        EvolutionOperation::Publish,
        EvolutionRecovery::Retry,
        "journal failed during evolution publication settlement",
    )
}
