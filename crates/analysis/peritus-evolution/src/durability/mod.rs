//! C0 persistence for evolution campaigns and the production-harness pointer.

mod activation;
mod binding;
mod campaign;
mod directive;
mod pointer;
mod replay;

pub use activation::{AtomicActivation, commit_atomic_activation};
pub use binding::{
    CAMPAIGN_STATE_NAMESPACE, POINTER_STATE_NAMESPACE, campaign_aggregate_key, campaign_state_key,
    pointer_aggregate_key, pointer_state_key,
};
pub use campaign::commit_campaign_transition;
pub use directive::{
    EVOLUTION_PUBLICATION_DESTINATION, EvolutionPublicationClaim, EvolutionPublicationDirective,
    EvolutionPublicationKind,
};
pub use pointer::commit_pointer_transition;
pub use replay::{CampaignReplay, PointerReplay, recover_campaign, recover_pointer};
