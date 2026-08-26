//! Deterministic crash-recovery choices from durable F0 observations.

use crate::{CampaignPhase, CampaignState, PointerPhase, ProductionHarnessState};

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
