//! Pure evolution-campaign transition tests.

use peritus_evolution::{
    CampaignCommand, CampaignCommandKind, CampaignPhase, EvolutionErrorKind, EvolutionOperation,
};
use peritus_types::{CommandId, EventId, EvolutionCampaignId, Sha256Digest};

const fn id(bytes: u8) -> [u8; 16] {
    [bytes; 16]
}

#[test]
fn command_constructor_rejects_a_head_without_sequence() {
    let error = CampaignCommand::new(
        CommandId::new(id(1)).expect("command id"),
        EventId::new(id(2)).expect("event id"),
        EvolutionCampaignId::new(id(3)).expect("campaign id"),
        0,
        Some(EventId::new(id(4)).expect("head id")),
        Sha256Digest::new([0; 32]),
        Sha256Digest::new([5; 32]),
        CampaignCommandKind::FreezeCampaign,
    )
    .expect_err("a genesis sequence cannot name a predecessor");

    assert_eq!(error.kind(), EvolutionErrorKind::InvalidInput);
    assert_eq!(error.operation(), EvolutionOperation::TransitionCampaign);
}

#[test]
fn only_truthful_campaign_outcomes_are_terminal() {
    for phase in [
        CampaignPhase::Draft,
        CampaignPhase::Frozen,
        CampaignPhase::BaselineRunning,
        CampaignPhase::Diagnosing,
        CampaignPhase::Proposing,
        CampaignPhase::VariantsRunning,
        CampaignPhase::Attributing,
        CampaignPhase::PromotionReview,
    ] {
        assert!(!phase.terminal());
    }
    for phase in [
        CampaignPhase::Promoted,
        CampaignPhase::Rejected,
        CampaignPhase::Failed,
        CampaignPhase::Cancelled,
    ] {
        assert!(phase.terminal());
    }
}
