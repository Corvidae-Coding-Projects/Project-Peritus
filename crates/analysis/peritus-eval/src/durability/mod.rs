//! C0 aggregate binding, outbox directives, atomic commits, and replay.

mod binding;
mod commit;
mod directive;
mod replay;

pub use binding::{EVALUATION_STATE_NAMESPACE, evaluation_aggregate_key, evaluation_state_key};
pub use commit::{
    commit_evaluation_claimed_transition, commit_evaluation_settlement,
    commit_evaluation_transition,
};
pub use directive::{
    EXECUTION_DESTINATION, EvaluationDirectiveClaim, ExecutionDirective, ExecutionDirectiveClaim,
    ExecutionDirectiveKind, PUBLICATION_DESTINATION, PublicationDirective,
    PublicationDirectiveClaim, SCHEDULE_DESTINATION, ScheduleDirective, ScheduleDirectiveClaim,
    ScheduleDirectiveKind,
};
pub use replay::{EvaluationReplay, load_evaluation_replay};
