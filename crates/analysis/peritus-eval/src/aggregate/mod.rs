//! Pure durable evaluation campaign aggregate.

mod command;
mod event;
mod reducer;
mod state;
mod types;

pub use command::{EvaluationCommand, EvaluationCommandKind};
pub use event::{EvaluationEvent, EvaluationEventKind, EvaluationTransition};
pub use reducer::{apply_event, decide, replay};
#[allow(
    clippy::redundant_pub_crate,
    reason = "shared with sibling wire and durability modules through the crate facade"
)]
pub(crate) use reducer::{encode_kind, encode_work};
pub use state::EvaluationState;
pub use types::{
    CampaignFailure, CampaignFailureCode, EvaluationPhase, PlanBatch, PlanRecord,
    PlannedRolloutBinding, PublicationRecord, ReportRecord, RolloutProgress, RolloutStatus,
    RolloutTerminalClass, TerminalRecordRef,
};
