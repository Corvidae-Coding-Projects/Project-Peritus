//! Bounded ownership and authority admission for interactive prompts.
//!
//! A3 validates wire syntax and prompt-local constraints. This module additionally binds every
//! response to its authenticated actor/session and live revision, and delegates approval decoding
//! and signature authentication to B1 without ever holding a human signing key.

mod approval;
mod clock;
mod error;
mod registry;
mod types;

pub use approval::CurrentApprovalAuthority;
pub use clock::AuthorityClock;
pub use error::{PromptBrokerError, PromptBrokerErrorKind};
pub use registry::PreparedPromptRegistration;
pub use registry::PromptBroker;
pub use types::{
    AuthenticatedApprovalResponse, PreparedPromptResponse, PromptAcceptance, PromptAdmission,
    PromptBrokerLimits, PromptCancellationAcceptance, PromptSettlementToken, PromptTerminalStatus,
};

#[cfg(test)]
mod tests;
