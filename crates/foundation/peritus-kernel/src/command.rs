//! Exhaustive lifecycle command vocabulary.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use peritus_policy::ActorRole;
use peritus_types::{
    ActionId, ActorId, AttemptId, EnvironmentId, FindingId, ReviewCycleId, RunId, Sha256Digest,
    TurnId,
};
use vstd::prelude::*;

verus! {

/// Stable command discriminant consumed by B3.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KernelCommandKind {
    PauseSession,
    ResumeSession,
    CloseSession,
    StartRun,
    PauseRun,
    ResumeRun,
    CancelRun,
    FailRun,
    ExhaustRun,
    RejectRun,
    StartAttempt,
    ResumeAttempt,
    SubmitAttempt,
    FailAttempt,
    ExhaustAttempt,
    StartTurn,
    CompleteTurn,
    FailTurn,
    CancelTurn,
    ProposeAction,
    AuthorizeAction,
    DispatchAction,
    CompleteAction,
    FailAction,
    CancelAction,
    RequestReview,
    BeginReview,
    SubmitReview,
    InvalidateReview,
    RequestWaiver,
    GrantWaiver,
    DenyWaiver,
    InvalidateWaiver,
    BeginAcceptance,
    EvaluateAcceptance,
}

/// Complete requests understood by the lifecycle kernel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KernelCommand {
    PauseSession,
    ResumeSession,
    CloseSession,
    StartRun { run_id: RunId },
    PauseRun { run_id: RunId },
    ResumeRun { run_id: RunId },
    CancelRun { run_id: RunId },
    FailRun { run_id: RunId },
    ExhaustRun { run_id: RunId },
    RejectRun { run_id: RunId },
    StartAttempt { run_id: RunId, attempt_id: AttemptId },
    ResumeAttempt { run_id: RunId, attempt_id: AttemptId },
    SubmitAttempt { run_id: RunId, attempt_id: AttemptId },
    FailAttempt { run_id: RunId, attempt_id: AttemptId },
    ExhaustAttempt { run_id: RunId, attempt_id: AttemptId },
    StartTurn { attempt_id: AttemptId, turn_id: TurnId },
    CompleteTurn { attempt_id: AttemptId, turn_id: TurnId },
    FailTurn { attempt_id: AttemptId, turn_id: TurnId },
    CancelTurn { attempt_id: AttemptId, turn_id: TurnId },
    ProposeAction {
        turn_id: TurnId,
        action_id: ActionId,
        digest: Sha256Digest,
        actor_id: ActorId,
        role: ActorRole,
        environment_id: EnvironmentId,
    },
    AuthorizeAction { action_id: ActionId },
    DispatchAction { action_id: ActionId },
    CompleteAction { action_id: ActionId },
    FailAction { action_id: ActionId },
    CancelAction { action_id: ActionId },
    RequestReview { run_id: RunId, attempt_id: AttemptId, review_id: ReviewCycleId },
    BeginReview { review_id: ReviewCycleId },
    SubmitReview { review_id: ReviewCycleId },
    InvalidateReview { review_id: ReviewCycleId },
    RequestWaiver {
        run_id: RunId,
        review_id: ReviewCycleId,
        finding_id: FindingId,
    },
    GrantWaiver { finding_id: FindingId },
    DenyWaiver { finding_id: FindingId },
    InvalidateWaiver { finding_id: FindingId },
    BeginAcceptance { run_id: RunId },
    EvaluateAcceptance { run_id: RunId },
}

impl KernelCommand {
    /// Returns the stable command discriminant.
    #[must_use]
    pub const fn kind(&self) -> KernelCommandKind {
        match self {
            Self::PauseSession => KernelCommandKind::PauseSession,
            Self::ResumeSession => KernelCommandKind::ResumeSession,
            Self::CloseSession => KernelCommandKind::CloseSession,
            Self::StartRun { .. } => KernelCommandKind::StartRun,
            Self::PauseRun { .. } => KernelCommandKind::PauseRun,
            Self::ResumeRun { .. } => KernelCommandKind::ResumeRun,
            Self::CancelRun { .. } => KernelCommandKind::CancelRun,
            Self::FailRun { .. } => KernelCommandKind::FailRun,
            Self::ExhaustRun { .. } => KernelCommandKind::ExhaustRun,
            Self::RejectRun { .. } => KernelCommandKind::RejectRun,
            Self::StartAttempt { .. } => KernelCommandKind::StartAttempt,
            Self::ResumeAttempt { .. } => KernelCommandKind::ResumeAttempt,
            Self::SubmitAttempt { .. } => KernelCommandKind::SubmitAttempt,
            Self::FailAttempt { .. } => KernelCommandKind::FailAttempt,
            Self::ExhaustAttempt { .. } => KernelCommandKind::ExhaustAttempt,
            Self::StartTurn { .. } => KernelCommandKind::StartTurn,
            Self::CompleteTurn { .. } => KernelCommandKind::CompleteTurn,
            Self::FailTurn { .. } => KernelCommandKind::FailTurn,
            Self::CancelTurn { .. } => KernelCommandKind::CancelTurn,
            Self::ProposeAction { .. } => KernelCommandKind::ProposeAction,
            Self::AuthorizeAction { .. } => KernelCommandKind::AuthorizeAction,
            Self::DispatchAction { .. } => KernelCommandKind::DispatchAction,
            Self::CompleteAction { .. } => KernelCommandKind::CompleteAction,
            Self::FailAction { .. } => KernelCommandKind::FailAction,
            Self::CancelAction { .. } => KernelCommandKind::CancelAction,
            Self::RequestReview { .. } => KernelCommandKind::RequestReview,
            Self::BeginReview { .. } => KernelCommandKind::BeginReview,
            Self::SubmitReview { .. } => KernelCommandKind::SubmitReview,
            Self::InvalidateReview { .. } => KernelCommandKind::InvalidateReview,
            Self::RequestWaiver { .. } => KernelCommandKind::RequestWaiver,
            Self::GrantWaiver { .. } => KernelCommandKind::GrantWaiver,
            Self::DenyWaiver { .. } => KernelCommandKind::DenyWaiver,
            Self::InvalidateWaiver { .. } => KernelCommandKind::InvalidateWaiver,
            Self::BeginAcceptance { .. } => KernelCommandKind::BeginAcceptance,
            Self::EvaluateAcceptance { .. } => KernelCommandKind::EvaluateAcceptance,
        }
    }
}

} // verus!
