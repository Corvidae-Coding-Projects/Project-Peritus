//! Independent full-value approval lifecycle oracle and observation projections.

#[path = "model/reducer.rs"]
mod reducer;
#[path = "model/views.rs"]
mod views;

use peritus_approval::{ActionDigest, AmendmentIdentity};
use peritus_policy::AuthorityInstant;
use peritus_types::ActionId;

pub use reducer::oracle_step;
pub use views::{
    AcceptedView, AggregateView, RejectedView, StepView, aggregate_view, amendment_view,
    initial_view, observation_view, transition_view, use_view,
};

#[derive(Clone, Copy, Debug)]
pub enum Command {
    ResolveApprove,
    ResolveDeny,
    ResolveAmend,
    Consume,
    ConsumeWrongDigest,
    Amend,
    AmendWrongCandidate,
    ExpireEarly,
    Expire,
    Cancel,
}

#[derive(Clone, Copy, Debug)]
pub struct InputView {
    pub command: Command,
    pub observation: Option<views::ObservationView>,
    pub action_id: ActionId,
    pub action_digest: ActionDigest,
    pub candidate: AmendmentIdentity,
    pub observed_at: AuthorityInstant,
}
