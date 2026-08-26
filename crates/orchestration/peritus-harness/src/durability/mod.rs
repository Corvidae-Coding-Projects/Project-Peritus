//! C0 binding, atomic commits, and checked recovery loading.

mod binding;
mod commit;
mod error;
mod recovery;

pub use binding::{HARNESS_STATE_NAMESPACE, harness_aggregate_key, harness_state_key};
pub use commit::{DirectiveClaim, commit_harness_settlement, commit_harness_transition};
pub use error::{DurabilityError, DurabilityErrorKind, DurabilityRecovery};
pub use recovery::{HarnessReplay, load_harness_replay};
