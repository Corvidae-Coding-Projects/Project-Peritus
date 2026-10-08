//! Provenance-checked evidence admission for claimed F0 publication directives.

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, ErrorCode as ArtifactErrorCode,
    RecoveryClass as ArtifactRecoveryClass,
};
use peritus_evidence::{
    EvidenceDraft, EvidenceKind, EvidenceRecord, EvidenceSource, EvidenceStore,
    RecoveryAction as EvidenceRecoveryAction,
};
use peritus_journal::{
    OutboxDeliveryStatus, RecoveryClass as JournalRecoveryClass, SqliteJournal,
};

use crate::{
    ActivationKind, CampaignCommand, CampaignCommandKind, CampaignEventKind, CampaignPublication,
    CampaignState, DurableActivationHistory, EvolutionError, EvolutionErrorKind,
    EvolutionOperation, EvolutionPublicationClaim, EvolutionPublicationKind,
    EvolutionPublicationObligation, EvolutionRecovery, FinalizedEvolutionArtifact,
    recover_activation_history, recover_campaign, resolve_campaign_receipt,
};

use super::{
    DurableEvolutionRecoveryObservation, EvolutionPublicationDependencyStatus,
    EvolutionPublicationDirectiveObservation, EvolutionPublicationOwnerObservation,
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

    /// Constructs the exact F0 campaign receipt for this verified owner publication.
    ///
    /// # Errors
    /// Rejects a harness-activation obligation or evidence that differs from the obligation.
    pub fn campaign_publication(
        &self,
        obligation: EvolutionPublicationObligation,
    ) -> Result<CampaignPublication, EvolutionError> {
        let directive = obligation.directive();
        if directive.kind() != EvolutionPublicationKind::CampaignDecision
            || self.evidence.id() != obligation.evidence_id()
            || self.evidence.revision() != &directive.revision()
            || self.evidence.provenance().global_position() != obligation.producing_position()
            || self.evidence.payload_digest() != directive.evidence_digest()
            || !self
                .evidence
                .artifacts()
                .contains(&ArtifactDigest::from_sha256(directive.artifact_digest()))
        {
            return Err(binding("published evidence differs from the campaign obligation"));
        }
        CampaignPublication::new(
            directive.artifact_digest(),
            obligation.evidence_id(),
            self.evidence.record_digest(),
            obligation.producing_position(),
        )
    }
}

/// Observes every publication required by immutable campaign and activation history.
///
/// Missing owner content is reported explicitly. Malformed or identity-divergent content is
/// rejected instead of being treated as absent.
///
/// # Errors
/// Rejects state/history drift, invalid retained receipts, or terminal owner-integrity failures.
pub fn observe_evolution_publication_recovery(
    journal: &SqliteJournal,
    evidence_store: &EvidenceStore,
    artifact_store: &ArtifactStore,
    campaign: &CampaignState,
    activation_history: &DurableActivationHistory,
    observed_at: u64,
) -> Result<DurableEvolutionRecoveryObservation, EvolutionError> {
    if observed_at == 0
        || activation_history.store_id() != journal.store_id()
        || activation_history.project_id() != campaign.project_id()
    {
        return Err(binding("publication recovery owner or observation tick differs"));
    }
    let recovered_history = recover_activation_history(journal, campaign.project_id())?;
    if &recovered_history != activation_history {
        return Err(binding("publication recovery activation history differs from C0"));
    }
    let mut obligations = Vec::new();
    if let Some(obligation) = campaign_obligation(journal, campaign)? {
        obligations.push(obligation);
    }
    for origin in activation_history.origins() {
        if origin.activation().kind() == ActivationKind::Initialization {
            continue;
        }
        let batch = journal
            .command_batch(origin.command_id())
            .map_err(journal_error)?
            .ok_or_else(|| binding("activation publication has no producing command receipt"))?;
        let origin_matches = batch.records().iter().any(|record| {
            record.global_position() == origin.global_position()
                && record.event_id() == origin.event_id()
                && record.command_id() == origin.command_id()
        });
        if !origin_matches || batch.last_position() != origin.checkpoint_position() {
            return Err(binding(
                "activation publication differs from its immutable producing transaction",
            ));
        }
        obligations.push(EvolutionPublicationObligation::new(
            origin.command_id(),
            batch.last_position(),
            crate::durability::activation_publication_directive(
                campaign.project_id(),
                origin.activation(),
            ),
        )?);
    }
    let mut publications = Vec::with_capacity(obligations.len());
    for obligation in obligations {
        publications.push(observe_obligation(
            journal,
            evidence_store,
            artifact_store,
            obligation,
            observed_at,
        )?);
    }
    Ok(DurableEvolutionRecoveryObservation::new(
        publications,
        campaign.publication(),
    ))
}

/// Restores one missing persistent intent from its original immutable producing transaction.
///
/// # Errors
/// Rejects command, payload, position, or delivery-policy drift and journal failures.
pub fn reconcile_evolution_publication_intent(
    journal: &mut SqliteJournal,
    obligation: EvolutionPublicationObligation,
    observed_at: u64,
) -> Result<OutboxDeliveryStatus, EvolutionError> {
    if observed_at == 0 {
        return Err(binding("publication reconciliation tick is zero"));
    }
    let batch = journal
        .command_batch(obligation.command_id())
        .map_err(journal_error)?
        .ok_or_else(|| binding("publication reconciliation command receipt is absent"))?;
    if batch.last_position() != obligation.producing_position() {
        return Err(binding(
            "publication reconciliation command has another producing position",
        ));
    }
    let draft = crate::durability::outbox_draft(obligation.directive())?;
    let message = journal.reconcile_outbox(&batch, &draft).map_err(journal_error)?;
    if !obligation.matches_message(&message) {
        return Err(binding("reconciled publication intent differs from its obligation"));
    }
    message.delivery_status(observed_at).map_err(journal_error)
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
    artifact_store.verify(artifact.artifact_digest()).map_err(artifact_error)?;
    let export = journal.integrity_export().map_err(journal_error)?;
    let artifacts = export
        .artifact_references()
        .iter()
        .filter(|reference| {
            reference.first_position() <= claim.producing_position()
                && claim.producing_position() <= reference.last_position()
        })
        .map(|reference| ArtifactDigest::from_sha256(reference.artifact_digest()))
        .collect::<Vec<_>>();
    if !artifacts.contains(&artifact.artifact_digest()) {
        return Err(binding("publication artifact is absent from the producing journal batch"));
    }
    let evidence_id = crate::durability::publication_evidence_id(
        claim.id(),
        claim.producing_position(),
    )?;
    let kind = match directive.kind() {
        EvolutionPublicationKind::CampaignDecision => "evolution-decision",
        EvolutionPublicationKind::HarnessActivation => "harness-activation",
    };
    let draft = EvidenceDraft::new(
        evidence_id,
        EvidenceKind::new(kind).map_err(evidence_error)?,
        EvidenceSource::new("peritus-evolution").map_err(evidence_error)?,
        directive.revision(),
        claim.producing_position(),
        directive.evidence_digest(),
        artifacts,
        Vec::new(),
    )
    .map_err(evidence_error)?;
    let evidence = evidence_store.admit(draft, &export, artifact_store).map_err(evidence_error)?;
    journal.acknowledge_outbox(claim.id(), claim.fence()).map_err(journal_error)?;
    Ok(EvolutionPublication { evidence })
}

fn campaign_obligation(
    journal: &SqliteJournal,
    campaign: &CampaignState,
) -> Result<Option<EvolutionPublicationObligation>, EvolutionError> {
    let Some(proposal) = campaign.proposal() else {
        return Ok(None);
    };
    let replay = recover_campaign(journal, campaign.campaign_id())?;
    if replay.state() != Some(campaign) {
        return Err(binding("campaign publication state differs from C0 replay"));
    }
    let event = replay
        .events()
        .iter()
        .find(|event| {
            matches!(
                event.kind(),
                CampaignEventKind::Accepted(CampaignCommandKind::RequestPromotion(observed))
                    if observed == proposal
            )
        })
        .ok_or_else(|| binding("campaign proposal has no immutable request event"))?;
    let CampaignEventKind::Accepted(kind) = event.kind();
    let expected_sequence = event
        .sequence()
        .checked_sub(1)
        .ok_or_else(|| binding("campaign publication event has zero sequence"))?;
    let command = CampaignCommand::new(
        event.command_id(),
        event.id(),
        event.campaign_id(),
        expected_sequence,
        event.previous_event(),
        event.prior_state_digest(),
        event.policy_digest(),
        kind.clone(),
    )?;
    let batch = resolve_campaign_receipt(journal, &command)?
        .ok_or_else(|| binding("campaign publication command receipt is absent"))?;
    Ok(Some(EvolutionPublicationObligation::new(
        event.command_id(),
        batch.last_position(),
        crate::durability::campaign_publication_directive(proposal),
    )?))
}

fn observe_obligation(
    journal: &SqliteJournal,
    evidence_store: &EvidenceStore,
    artifact_store: &ArtifactStore,
    obligation: EvolutionPublicationObligation,
    observed_at: u64,
) -> Result<EvolutionPublicationOwnerObservation, EvolutionError> {
    let directive = obligation.directive();
    let artifact = match artifact_store
        .verify(ArtifactDigest::from_sha256(directive.artifact_digest()))
    {
        Ok(_) => EvolutionPublicationDependencyStatus::Verified,
        Err(error) if error.code() == ArtifactErrorCode::MissingArtifact => {
            EvolutionPublicationDependencyStatus::Missing
        }
        Err(error) if error.recovery_class() == ArtifactRecoveryClass::Retry => {
            EvolutionPublicationDependencyStatus::Unavailable
        }
        Err(error) => return Err(artifact_error(error)),
    };
    let draft = publication_draft(journal, obligation)?;
    let (evidence, evidence_record_digest) = match evidence_store.load(obligation.evidence_id()) {
        Ok(Some(record)) => {
            validate_recovered_evidence(&record, &draft)?;
            (
                EvolutionPublicationDependencyStatus::Verified,
                Some(record.record_digest()),
            )
        }
        Ok(None) => (EvolutionPublicationDependencyStatus::Missing, None),
        Err(error) if error.recovery() == EvidenceRecoveryAction::Retry => {
            (EvolutionPublicationDependencyStatus::Unavailable, None)
        }
        Err(_) => return Err(binding("retained evolution evidence failed owner verification")),
    };
    let directive = match journal.outbox_message(obligation.outbox_id()) {
        Ok(Some(message)) => {
            if !obligation.matches_message(&message) {
                return Err(binding("retained publication directive differs from its obligation"));
            }
            match message.delivery_status(observed_at) {
                Ok(status) => EvolutionPublicationDirectiveObservation::Observed(status),
                Err(error) if error.recovery() == JournalRecoveryClass::Retry => {
                    EvolutionPublicationDirectiveObservation::Unavailable
                }
                Err(error) => return Err(journal_error(error)),
            }
        }
        Ok(None) => EvolutionPublicationDirectiveObservation::Missing,
        Err(error) if error.recovery() == JournalRecoveryClass::Retry => {
            EvolutionPublicationDirectiveObservation::Unavailable
        }
        Err(error) => return Err(journal_error(error)),
    };
    Ok(EvolutionPublicationOwnerObservation::new(
        obligation,
        artifact,
        evidence,
        evidence_record_digest,
        directive,
    ))
}

fn publication_draft(
    journal: &SqliteJournal,
    obligation: EvolutionPublicationObligation,
) -> Result<EvidenceDraft, EvolutionError> {
    let directive = obligation.directive();
    let export = journal.integrity_export().map_err(journal_error)?;
    let artifacts = export
        .artifact_references()
        .iter()
        .filter(|reference| {
            reference.first_position() <= obligation.producing_position()
                && obligation.producing_position() <= reference.last_position()
        })
        .map(|reference| ArtifactDigest::from_sha256(reference.artifact_digest()))
        .collect::<Vec<_>>();
    if !artifacts.contains(&ArtifactDigest::from_sha256(directive.artifact_digest())) {
        return Err(binding("publication artifact is absent from the producing journal batch"));
    }
    let kind = match directive.kind() {
        EvolutionPublicationKind::CampaignDecision => "evolution-decision",
        EvolutionPublicationKind::HarnessActivation => "harness-activation",
    };
    EvidenceDraft::new(
        obligation.evidence_id(),
        EvidenceKind::new(kind).map_err(evidence_error)?,
        EvidenceSource::new("peritus-evolution").map_err(evidence_error)?,
        directive.revision(),
        obligation.producing_position(),
        directive.evidence_digest(),
        artifacts,
        Vec::new(),
    )
    .map_err(evidence_error)
}

fn validate_recovered_evidence(
    evidence: &EvidenceRecord,
    draft: &EvidenceDraft,
) -> Result<(), EvolutionError> {
    if evidence.id() != draft.id()
        || evidence.kind() != draft.kind()
        || evidence.source() != draft.source()
        || evidence.revision() != draft.revision()
        || evidence.provenance().global_position() != draft.journal_position()
        || evidence.payload_digest() != draft.payload_digest()
        || evidence.artifacts() != draft.artifacts()
        || evidence.causes() != draft.causes()
    {
        return Err(binding("retained evolution evidence differs from its publication obligation"));
    }
    Ok(())
}

const fn binding(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::Publish,
        EvolutionRecovery::Quarantine,
        detail,
    )
}

fn artifact_error(_: impl core::fmt::Display) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Artifact,
        EvolutionOperation::Publish,
        EvolutionRecovery::Reconcile,
        "evolution publication artifact verification failed",
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
