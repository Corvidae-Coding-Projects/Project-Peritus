//! Durable deterministic causal collaboration for Peritus.

mod binding;
mod canonical;
mod command;
mod durability;
mod error;
mod event;
mod identity;
mod join;
mod limits;
mod message;
mod projection;
mod reducer;
mod replay;
mod state;
mod task;
mod verified;
mod wire;

pub use binding::CollaborationBinding;
pub use command::{CollaborationCommand, CollaborationCommandKind};
pub use durability::{
    COLLABORATION_STATE_NAMESPACE, commit_collaboration_transition, load_collaboration_replay,
};
pub use error::{CollaborationError, CollaborationErrorKind, CollaborationRecoveryAction};
pub use event::{CollaborationEvent, CollaborationEventKind, CollaborationTransition};
pub use identity::{CollaborationId, CollaborationMessageId, CollaborationTaskId};
pub use join::{ArtifactHandoff, JoinPolicy};
pub use limits::CollaborationLimits;
pub use message::{CollaborationMessage, MessageDelivery};
pub use projection::{CollaborationProjection, ProjectedMessage, ProjectedTask};
pub use reducer::{decide, replay, start};
pub use replay::CollaborationReplay;
pub use state::{
    CollaborationPhase, CollaborationState, CollaborationTerminal, CollaborationTerminalKind,
};
pub use task::{
    CancellationEffect, CollaborationTask, Delegation, ReservationObservation, TaskPhase,
    TaskTerminal, TaskTerminalKind,
};
pub use verified::{
    cancellation_dominates, causal_graph_is_valid, join_is_truthful, replay_equivalent,
    terminal_is_truthful, transition_is_legal,
};
