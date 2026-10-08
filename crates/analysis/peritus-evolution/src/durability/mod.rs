//! C0 persistence for evolution campaigns and the production-harness pointer.

mod activation;
mod binding;
mod campaign;
mod checkpoint;
mod directive;
mod pointer;
mod replay;

pub use activation::{
    AtomicActivation, commit_atomic_activation, commit_atomic_activation_with_storage,
};
pub use binding::{
    CAMPAIGN_STATE_NAMESPACE, POINTER_STATE_NAMESPACE, campaign_aggregate_key, campaign_state_key,
    pointer_aggregate_key, pointer_state_key,
};
pub use campaign::{
    commit_campaign_transition, commit_campaign_transition_with_storage, resolve_campaign_receipt,
};
pub use directive::{
    EVOLUTION_PUBLICATION_DESTINATION, EvolutionPublicationClaim, EvolutionPublicationDirective,
    EvolutionPublicationKind,
};
pub use pointer::{
    commit_pointer_transition, commit_pointer_transition_with_storage, prepare_rollback_proposal,
    recover_activation_history, resolve_activation_origin, resolve_pointer_receipt,
};
pub use replay::{CampaignReplay, PointerReplay, recover_campaign, recover_pointer};
