//! Long-lived project production-pointer aggregate and append-only rollback.

mod command;
mod event;
mod projection;
mod reducer;
mod rollback;
mod state;

pub use command::{PointerCommand, PointerCommandKind};
pub use event::{PointerEvent, PointerEventKind, PointerTransition};
pub use projection::ProductionHarnessProjection;
pub use reducer::{apply_pointer_event, decide_pointer, replay_pointer};
pub use rollback::{ActivationAuthorization, CompatibilityWitness, RollbackProposal};
pub(crate) use state::activation_record;
pub use state::{
    ActivationKind, ActivationRecord, PendingActivation, PointerPhase, ProductionHarnessState,
};
