//! Long-lived project production-pointer aggregate and append-only rollback.

mod command;
mod event;
mod history;
mod projection;
mod reducer;
mod rollback;
mod state;

pub use command::{PointerCommand, PointerCommandKind};
pub use event::{PointerEvent, PointerEventKind, PointerTransition};
pub use history::{DurableActivationHistory, DurableActivationOrigin};
pub use projection::ProductionHarnessProjection;
pub(crate) use reducer::apply_pointer_event_legacy_eviction;
pub use reducer::{apply_pointer_event, decide_pointer, replay_pointer};
pub use rollback::{ActivationAuthorization, CompatibilityWitness, RollbackProposal};
pub(crate) use state::activation_record;
pub use state::{
    ActivationKind, ActivationRecord, PendingActivation, PointerPhase, ProductionHarnessState,
};
