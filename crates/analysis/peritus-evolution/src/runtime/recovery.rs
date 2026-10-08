//! Deterministic crash-recovery choices from durable F0 observations.

use peritus_journal::OutboxDeliveryStatus;
use peritus_types::Sha256Digest;

use crate::{
    CampaignPhase, CampaignPublication, CampaignState, EvolutionPublicationKind,
    EvolutionPublicationObligation, PointerPhase, ProductionHarnessState,
};

/// Result of observing one exact publication dependency through its owning store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvolutionPublicationDependencyStatus {
    /// The exact identity exists and passed owner verification.
    Verified,
    /// The owner authoritatively reported that the exact identity is absent.
    Missing,
    /// The owner could not complete the read and the same observation may be retried.
    Unavailable,
}

/// Read-only observation of one exact persistent publication directive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvolutionPublicationDirectiveObservation {
    /// The exact retained row was decoded and bound to its original transaction.
    Observed(OutboxDeliveryStatus),
    /// The outbox owner authoritatively reported the canonical identity absent.
    Missing,
    /// The outbox owner could not complete the read.
    Unavailable,
}

/// Identity-bound owner observations for one required F0 publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvolutionPublicationOwnerObservation {
    obligation: EvolutionPublicationObligation,
    artifact: EvolutionPublicationDependencyStatus,
    evidence: EvolutionPublicationDependencyStatus,
    evidence_record_digest: Option<Sha256Digest>,
    directive: EvolutionPublicationDirectiveObservation,
}

impl EvolutionPublicationOwnerObservation {
    pub(crate) const fn new(
        obligation: EvolutionPublicationObligation,
        artifact: EvolutionPublicationDependencyStatus,
        evidence: EvolutionPublicationDependencyStatus,
        evidence_record_digest: Option<Sha256Digest>,
        directive: EvolutionPublicationDirectiveObservation,
    ) -> Self {
        Self { obligation, artifact, evidence, evidence_record_digest, directive }
    }
    /// Exact immutable publication requirement.
    #[must_use]
    pub const fn obligation(self) -> EvolutionPublicationObligation {
        self.obligation
    }
    /// Artifact-owner result.
    #[must_use]
    pub const fn artifact(self) -> EvolutionPublicationDependencyStatus {
        self.artifact
    }
    /// Evidence-owner result.
    #[must_use]
    pub const fn evidence(self) -> EvolutionPublicationDependencyStatus {
        self.evidence
    }
    /// Retained evidence-record digest when the exact record verified.
    #[must_use]
    pub const fn evidence_record_digest(self) -> Option<Sha256Digest> {
        self.evidence_record_digest
    }
    /// Transport-owner result, kept separate from publication truth.
    #[must_use]
    pub const fn directive(self) -> EvolutionPublicationDirectiveObservation {
        self.directive
    }
}

/// Complete identity-bound publication observations for one recovery decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableEvolutionRecoveryObservation {
    publications: Vec<EvolutionPublicationOwnerObservation>,
    campaign_publication: Option<CampaignPublication>,
}

impl DurableEvolutionRecoveryObservation {
    pub(crate) const fn new(
        publications: Vec<EvolutionPublicationOwnerObservation>,
        campaign_publication: Option<CampaignPublication>,
    ) -> Self {
        Self { publications, campaign_publication }
    }
    /// Every publication obligation derived from immutable campaign and activation history.
    #[must_use]
    pub fn publications(&self) -> &[EvolutionPublicationOwnerObservation] {
        &self.publications
    }
    /// Campaign-retained decision-publication receipt, when settled into F0 state.
    #[must_use]
    pub const fn campaign_publication(&self) -> Option<CampaignPublication> {
        self.campaign_publication
    }
}

/// Exact external observations; none grants mutation authority.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent durable observations are intentionally not collapsed into inferred state"
)]
pub struct EvolutionRecoveryObservation {
    /// Exact publication directive remains unacknowledged.
    pub publication_directive: bool,
    /// Referenced artifact exists and verifies.
    pub artifact_verified: bool,
    /// Matching evidence record is already admitted.
    pub evidence_admitted: bool,
    /// External identity ownership conflicts with durable truth.
    pub identity_conflict: bool,
}

/// Closed recovery action selected without guessing effect outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvolutionRecoveryDecision {
    /// Continue normal pure campaign processing.
    ContinueCampaign,
    /// Await a new or existing matching human approval.
    AwaitAuthority,
    /// Retry the existing exact publication directive.
    RetryPublication,
    /// Artifact finalization or verification must be reconciled.
    ReconcileArtifact,
    /// Evidence exists; idempotently settle its publication claim.
    ReconcileEvidence,
    /// Restore the exact persistent directive from its immutable producing receipt.
    ReconcilePublicationIntent,
    /// Repeat owner observation after temporary store unavailability.
    AwaitPublicationObservation,
    /// Durable terminal and publication state is complete.
    Complete,
    /// Contradictory ownership requires operator quarantine.
    Quarantine,
}

/// Selects the exact recovery action for a campaign and production pointer.
#[must_use]
pub fn decide_recovery(
    campaign: &CampaignState,
    pointer: &ProductionHarnessState,
    observed: EvolutionRecoveryObservation,
) -> EvolutionRecoveryDecision {
    if observed.identity_conflict {
        return EvolutionRecoveryDecision::Quarantine;
    }
    if observed.publication_directive {
        if !observed.artifact_verified {
            return EvolutionRecoveryDecision::ReconcileArtifact;
        }
        if observed.evidence_admitted {
            return EvolutionRecoveryDecision::ReconcileEvidence;
        }
        return EvolutionRecoveryDecision::RetryPublication;
    }
    if let Some(proposal) = campaign.proposal() {
        if !observed.artifact_verified {
            return EvolutionRecoveryDecision::ReconcileArtifact;
        }
        if !observed.evidence_admitted {
            return EvolutionRecoveryDecision::ReconcileEvidence;
        }
        match campaign.publication() {
            Some(publication)
                if publication.artifact_digest() == proposal.evidence_bundle_artifact() => {}
            Some(_) => return EvolutionRecoveryDecision::Quarantine,
            None => return EvolutionRecoveryDecision::ReconcileEvidence,
        }
    }
    if pointer.phase() != PointerPhase::Active || campaign.phase() == CampaignPhase::PromotionReview
    {
        return EvolutionRecoveryDecision::AwaitAuthority;
    }
    if campaign.phase().terminal() {
        EvolutionRecoveryDecision::Complete
    } else {
        EvolutionRecoveryDecision::ContinueCampaign
    }
}

/// Selects recovery only after every durable publication obligation has been owner-verified.
#[must_use]
pub fn decide_durable_recovery(
    campaign: &CampaignState,
    pointer: &ProductionHarnessState,
    observed: &DurableEvolutionRecoveryObservation,
) -> EvolutionRecoveryDecision {
    let mut campaign_observed = campaign.proposal().is_none();
    let mut activation_observed = campaign.phase() != CampaignPhase::Promoted;
    for publication in observed.publications() {
        let obligation = publication.obligation();
        match obligation.directive().kind() {
            EvolutionPublicationKind::CampaignDecision => campaign_observed = true,
            EvolutionPublicationKind::HarnessActivation
                if obligation.directive().campaign_id() == Some(campaign.campaign_id()) =>
            {
                activation_observed = true;
            }
            EvolutionPublicationKind::HarnessActivation => {}
        }
        if publication.artifact() == EvolutionPublicationDependencyStatus::Unavailable
            || publication.evidence() == EvolutionPublicationDependencyStatus::Unavailable
            || publication.directive() == EvolutionPublicationDirectiveObservation::Unavailable
        {
            return EvolutionRecoveryDecision::AwaitPublicationObservation;
        }
        if publication.artifact() == EvolutionPublicationDependencyStatus::Missing {
            return EvolutionRecoveryDecision::ReconcileArtifact;
        }
        match publication.directive() {
            EvolutionPublicationDirectiveObservation::Missing => {
                return EvolutionRecoveryDecision::ReconcilePublicationIntent;
            }
            EvolutionPublicationDirectiveObservation::Observed(OutboxDeliveryStatus::Exhausted) => {
                return EvolutionRecoveryDecision::Quarantine;
            }
            EvolutionPublicationDirectiveObservation::Observed(OutboxDeliveryStatus::Acknowledged)
                if publication.evidence() == EvolutionPublicationDependencyStatus::Missing =>
            {
                return EvolutionRecoveryDecision::ReconcileEvidence;
            }
            EvolutionPublicationDirectiveObservation::Observed(_) 
                if publication.evidence() == EvolutionPublicationDependencyStatus::Missing =>
            {
                return EvolutionRecoveryDecision::RetryPublication;
            }
            EvolutionPublicationDirectiveObservation::Observed(OutboxDeliveryStatus::Acknowledged) => {}
            EvolutionPublicationDirectiveObservation::Observed(_) => {
                return EvolutionRecoveryDecision::ReconcileEvidence;
            }
            EvolutionPublicationDirectiveObservation::Unavailable => {
                return EvolutionRecoveryDecision::AwaitPublicationObservation;
            }
        }
        if obligation.directive().kind() == EvolutionPublicationKind::CampaignDecision {
            let Some(receipt) = observed.campaign_publication() else {
                return EvolutionRecoveryDecision::ReconcileEvidence;
            };
            if receipt.artifact_digest() != obligation.directive().artifact_digest()
                || receipt.evidence_id() != obligation.evidence_id()
                || receipt.journal_position() != obligation.producing_position()
                || Some(receipt.evidence_digest()) != publication.evidence_record_digest()
            {
                return EvolutionRecoveryDecision::Quarantine;
            }
        }
    }
    if !campaign_observed || !activation_observed {
        return EvolutionRecoveryDecision::Quarantine;
    }
    if pointer.phase() != PointerPhase::Active || campaign.phase() == CampaignPhase::PromotionReview
    {
        EvolutionRecoveryDecision::AwaitAuthority
    } else if campaign.phase().terminal() {
        EvolutionRecoveryDecision::Complete
    } else {
        EvolutionRecoveryDecision::ContinueCampaign
    }
}
