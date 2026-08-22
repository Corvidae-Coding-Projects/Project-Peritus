//! Occurrence-addressed, protocol-neutral deterministic fault injection.

mod failure;
mod injector;
mod name;
mod plan;

pub use failure::{FaultControlError, FaultNameError, FaultPlanError, FaultVerificationError};
pub use injector::{FaultInjector, FaultSnapshot};
pub use name::{FaultLabel, FaultPoint};
pub use plan::{FaultExpectation, FaultHit, FaultPlan};
