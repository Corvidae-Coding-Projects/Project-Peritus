//! Bounded ownership and authority admission for interactive prompts.
//!
//! A3 validates wire syntax and prompt-local constraints. This module additionally binds every
//! response to its authenticated actor/session and live revision, and delegates approval decoding
//! and signature authentication to B1 without ever holding a human signing key.

mod approval;
mod error;
mod registry;
mod types;

pub use approval::CurrentApprovalAuthority;
pub use error::{PromptBrokerError, PromptBrokerErrorKind};
pub use registry::PromptBroker;
pub use types::{
    AuthenticatedApprovalResponse, PromptAcceptance, PromptAdmission, PromptBrokerLimits,
    PromptCancellationAcceptance, PromptTerminalStatus,
};
