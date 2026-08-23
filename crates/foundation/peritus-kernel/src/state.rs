//! Closed lifecycle phases and checked state records.

use vstd::prelude::*;

verus! {

mod acceptance;
mod action;
mod attempt;
mod review;
mod run;
mod session;
mod turn;
mod waiver;

pub use acceptance::AcceptancePhase;
pub use action::{ActionPhase, ActionState};
pub use attempt::{AttemptPhase, AttemptState};
pub use review::{ReviewPhase, ReviewState};
pub use run::{RunPhase, RunState};
pub use session::{SessionPhase, SessionState};
pub use turn::{TurnPhase, TurnState};
pub use waiver::{WaiverPhase, WaiverState};

} // verus!
