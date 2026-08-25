//! Durable deterministic bounded resource scheduling for Peritus.

pub(crate) mod binding;
pub(crate) mod canonical;
pub(crate) mod command;
pub(crate) mod durability;
pub(crate) mod error;
pub(crate) mod event;
pub(crate) mod identity;
pub(crate) mod limits;
pub(crate) mod projection;
pub(crate) mod reducer;
pub(crate) mod resource;
pub(crate) mod runtime;
pub(crate) mod selection;
pub(crate) mod state;
pub(crate) mod verified;
pub(crate) mod wire;
pub(crate) mod work;
pub(crate) mod worker;

pub use binding::SchedulerBinding;
pub use command::{FailureDisposition, SchedulerCommand, SchedulerCommandKind};
pub use durability::{
    SCHEDULER_STATE_NAMESPACE, SchedulerReplay, commit_scheduler_transition, load_scheduler_replay,
    scheduler_aggregate_key, scheduler_state_key,
};
pub use error::{SchedulerError, SchedulerErrorKind, SchedulerRecoveryAction};
pub use event::{LossOutcome, SchedulerEvent, SchedulerEventKind, SchedulerTransition};
pub use identity::{DispatchId, SchedulerId, WorkId, WorkerId};
pub use limits::{AttemptNumber, SchedulerLimits};
pub use projection::{
    ProjectedReservation, ProjectedScheduler, ProjectedWork, ProjectedWorker, SchedulerProjection,
};
pub use reducer::{decide, replay, start};
pub use resource::{ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector};
pub use runtime::{SchedulerDirective, pending_directives};
pub use selection::{Selection, select_next};
pub use state::{SchedulerPhase, SchedulerState, SchedulerTerminal, SchedulerTerminalKind};
pub use verified::{
    attempts_are_monotonic, dependencies_are_ready, no_implicit_success, replay_equivalent,
    reservations_fit, transition_is_legal, unique_dispatch_ownership,
};
pub use wire::{SchedulerCommandFrame, SchedulerEventFrame, SchedulerStateFrame};
pub use work::{ExecutionClass, RecoveryPolicy, WorkPhase, WorkRecord, WorkSpec, WorkTerminal};
pub use worker::{SchedulerReservation, WorkerDescriptor, WorkerPhase, WorkerRecord};
