//! Immutable evolution-campaign aggregate and exact promotion proposals.

mod command;
mod event;
mod projection;
mod proposal;
mod reducer;
mod state;

pub use command::{CampaignCommand, CampaignCommandKind};
pub use event::{CampaignEvent, CampaignEventKind, CampaignTransition};
pub use projection::EvolutionProjection;
pub use proposal::{CampaignPublication, PromotionProposal};
pub use reducer::{apply_campaign_event, decide_campaign, replay_campaign};
pub use state::{
    BaselineEvidence, CampaignPhase, CampaignState, CampaignTerminal, VariantEvaluation,
};
