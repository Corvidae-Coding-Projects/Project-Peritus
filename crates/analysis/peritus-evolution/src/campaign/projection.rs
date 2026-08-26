//! Rebuildable read model for one evolution campaign.

use peritus_types::{ProjectId, Sha256Digest};

use crate::{
    CampaignPhase, CampaignState, CampaignTerminal, EvolutionCampaignId, PromotionId, VariantId,
};

/// Compact non-authoritative campaign query projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvolutionProjection {
    campaign_id: EvolutionCampaignId,
    project_id: ProjectId,
    phase: CampaignPhase,
    sequence: u64,
    variant_count: u32,
    evaluation_count: u32,
    selected_variant: Option<VariantId>,
    promotion_id: Option<PromotionId>,
    terminal: Option<CampaignTerminal>,
    publication_pending: bool,
    state_digest: Sha256Digest,
}

impl EvolutionProjection {
    /// Rebuilds a query value solely from authoritative replayed state.
    #[must_use]
    pub fn from_state(state: &CampaignState) -> Self {
        let selected_variant = state.proposal().map(crate::PromotionProposal::variant_id);
        Self {
            campaign_id: state.campaign_id(),
            project_id: state.project_id(),
            phase: state.phase(),
            sequence: state.sequence(),
            variant_count: u32::try_from(state.variants().len()).unwrap_or(u32::MAX),
            evaluation_count: u32::try_from(state.evaluations().len()).unwrap_or(u32::MAX),
            selected_variant,
            promotion_id: state.proposal().map(crate::PromotionProposal::id),
            terminal: state.terminal(),
            publication_pending: state.terminal().is_some() && state.publication().is_none(),
            state_digest: state.state_digest(),
        }
    }
    /// Campaign identity.
    #[must_use]
    pub const fn campaign_id(self) -> EvolutionCampaignId {
        self.campaign_id
    }
    /// Project authority.
    #[must_use]
    pub const fn project_id(self) -> ProjectId {
        self.project_id
    }
    /// Current lifecycle phase.
    #[must_use]
    pub const fn phase(self) -> CampaignPhase {
        self.phase
    }
    /// Applied event sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
    /// Number of admitted variants.
    #[must_use]
    pub const fn variant_count(self) -> u32 {
        self.variant_count
    }
    /// Number of admitted E3 evaluations.
    #[must_use]
    pub const fn evaluation_count(self) -> u32 {
        self.evaluation_count
    }
    /// Selected variant when a promotion was proposed.
    #[must_use]
    pub const fn selected_variant(self) -> Option<VariantId> {
        self.selected_variant
    }
    /// Frozen promotion identity.
    #[must_use]
    pub const fn promotion_id(self) -> Option<PromotionId> {
        self.promotion_id
    }
    /// Truthful terminal result.
    #[must_use]
    pub const fn terminal(self) -> Option<CampaignTerminal> {
        self.terminal
    }
    /// Whether terminal evidence still needs an acknowledgement.
    #[must_use]
    pub const fn publication_pending(self) -> bool {
        self.publication_pending
    }
    /// Complete authoritative state digest.
    #[must_use]
    pub const fn state_digest(self) -> Sha256Digest {
        self.state_digest
    }
}
