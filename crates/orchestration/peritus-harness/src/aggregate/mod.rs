//! Pure harness command, event, state, and reducer model.

mod command;
mod error;
mod event;
mod reducer;
mod state;

pub use command::{HarnessCommand, HarnessCommandKind, ReconciliationDecision};
pub use error::{AggregateError, AggregateErrorKind, AggregateRecovery};
pub use event::{HarnessEvent, HarnessEventKind, HarnessTransition};
pub(crate) use reducer::apply_event;
pub use reducer::decide;
pub use state::{DeliveryState, HarnessState, PendingMaterialization};
