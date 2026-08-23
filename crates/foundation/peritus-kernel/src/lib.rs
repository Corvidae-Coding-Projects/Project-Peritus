//! Verified lifecycle state machines and authoritative event reduction for Peritus.
//!
//! Commands are effect-free requests. Successful reduction produces one logical next-state/event
//! plan; it is not a durable commit receipt and grants no ambient effect authority.

use vstd::prelude::*;

verus! {

mod aggregate;
mod authorization;
mod command;
mod envelope;
mod error;
mod event;
mod inputs;
mod identity;
#[cfg(verus_only)]
mod model;
#[cfg(verus_only)]
mod proofs;
mod reducer;
mod state;
mod transition;

pub use aggregate::{KernelAggregate, KernelGenesis};
pub use authorization::ActionAuthorizationWitness;
pub use command::{KernelCommand, KernelCommandKind};
pub use envelope::CommandEnvelope;
pub use error::{AuthorityInputKind, KernelError, KernelErrorKind, LifecycleEntity};
pub use event::{KernelEvent, KernelEventKind, KernelSubject};
pub use inputs::ReducerInputs;
pub use state::{
    AcceptancePhase, ActionPhase, ActionState, AttemptPhase, AttemptState, ReviewPhase,
    ReviewState, RunPhase, RunState, SessionPhase, SessionState, TurnPhase, TurnState,
    WaiverPhase, WaiverState,
};
pub use transition::{AcceptanceOutcome, KernelOutcome, KernelTransition};

} // verus!
