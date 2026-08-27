//! Opaque bounded B1 approval authority frames carried by A3.

use peritus_types::{CommandId, RevisionNumber};

use super::{PromptError, PromptErrorKind, error::reject};

/// Exact B1 approval request plus the daemon-reserved decision identity and registry revision.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ApprovalChallenge {
    decision_command_id: CommandId,
    registry_revision: RevisionNumber,
    request_frame: Vec<u8>,
}

impl ApprovalChallenge {
    /// Creates a challenge carrying one nonempty bounded canonical B1 request frame.
    ///
    /// # Errors
    ///
    /// Rejects a zero bound or an empty/oversized frame.
    pub fn new(
        decision_command_id: CommandId,
        registry_revision: RevisionNumber,
        request_frame: Vec<u8>,
        maximum_frame_bytes: usize,
    ) -> Result<Self, PromptError> {
        bounded_frame(&request_frame, maximum_frame_bytes)?;
        Ok(Self { decision_command_id, registry_revision, request_frame })
    }

    /// Returns the B3 command identity reserved for the signed decision.
    #[must_use]
    pub const fn decision_command_id(&self) -> CommandId {
        self.decision_command_id
    }

    /// Returns the exact credential-registry revision challenged by the daemon.
    #[must_use]
    pub const fn registry_revision(&self) -> RevisionNumber {
        self.registry_revision
    }

    /// Borrows the opaque canonical B1 approval request frame.
    #[must_use]
    pub fn request_frame(&self) -> &[u8] {
        &self.request_frame
    }
}

/// One nonempty bounded canonical B1 signed-approval-decision frame.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SignedApprovalDecisionFrame(Vec<u8>);

impl SignedApprovalDecisionFrame {
    /// Stores opaque signed-decision bytes without interpreting authority.
    ///
    /// # Errors
    ///
    /// Rejects a zero bound or an empty/oversized frame.
    pub fn new(bytes: Vec<u8>, maximum_frame_bytes: usize) -> Result<Self, PromptError> {
        bounded_frame(&bytes, maximum_frame_bytes)?;
        Ok(Self(bytes))
    }

    /// Borrows the exact opaque canonical B1 signed-decision frame.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Closed approval response transported by A3 without granting authority.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ApprovalAnswer {
    /// The client supplies an externally signed complete B1 decision.
    SignedDecision(SignedApprovalDecisionFrame),
    /// The client cancels the interaction without creating a decision.
    Cancel,
}

const fn bounded_frame(bytes: &[u8], maximum: usize) -> Result<(), PromptError> {
    if maximum == 0 {
        return Err(reject(PromptErrorKind::InvalidLimit, "approval frame limit is zero"));
    }
    if bytes.is_empty() || bytes.len() > maximum {
        return Err(reject(
            PromptErrorKind::InvalidInput,
            "approval frame is empty or exceeds its negotiated bound",
        ));
    }
    Ok(())
}
