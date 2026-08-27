//! Correlated, revision-fresh prompt values and pure admission state.

mod approval;
mod binding;
mod error;
mod response;
mod state;

pub use approval::{ApprovalAnswer, ApprovalChallenge, SignedApprovalDecisionFrame};
pub use binding::{PromptBinding, PromptChoice, PromptConstraint, PromptCorrelation, PromptKind};
pub use error::{PromptError, PromptErrorKind};
pub use response::{PromptAnswer, PromptAnswerPayload, PromptCancellation, UserInputValue};
pub use state::{PromptPhase, PromptState};
