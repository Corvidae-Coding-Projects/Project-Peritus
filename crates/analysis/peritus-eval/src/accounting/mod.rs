//! Complete rollout attempt, terminal, and resource accounting.

mod ledger;
mod outcome;
mod record;
mod resource;

pub use ledger::{LedgerCounts, RolloutLedger};
pub use outcome::{InfrastructureFailureClass, RolloutAttempt, RolloutOutcome, TaskFailureClass};
pub use record::RolloutRecord;
pub use resource::ResourceObservation;
