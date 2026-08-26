//! Closed typed application control envelope.

use crate::{
    Acknowledgement, ArtifactCancellation, CorrelationId, HeartbeatId, PauseReason,
    PromptCancellation, SubscriptionCancellation, SubscriptionId, TerminalCancellation,
};

use super::ProtocolContext;

/// Subscription pause/resume control.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SubscriptionControl {
    /// Pauses delivery for an explicit reason.
    Pause {
        /// Subscription whose deliveries are paused.
        subscription_id: SubscriptionId,
        /// Explicit reason recorded for the pause.
        reason: PauseReason,
    },
    /// Resumes a paused subscription.
    Resume {
        /// Subscription whose deliveries resume.
        subscription_id: SubscriptionId,
    },
}

/// Heartbeat reply echoing the exact nonce and sequence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HeartbeatReply {
    heartbeat_id: HeartbeatId,
    sequence: u64,
}

impl HeartbeatReply {
    /// Creates an exact heartbeat reply.
    #[must_use]
    pub const fn new(heartbeat_id: HeartbeatId, sequence: u64) -> Self {
        Self { heartbeat_id, sequence }
    }
    /// Returns the echoed heartbeat nonce.
    #[must_use]
    pub const fn heartbeat_id(self) -> HeartbeatId {
        self.heartbeat_id
    }
    /// Returns the echoed heartbeat sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
}

/// Closed schema-v1 application control payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControlPayload {
    /// Cumulative event acknowledgement.
    Acknowledge(Acknowledgement),
    /// Correlated subscription cancellation.
    CancelSubscription(SubscriptionCancellation),
    /// Correlated artifact cancellation.
    CancelArtifact(ArtifactCancellation),
    /// Correlated prompt cancellation.
    CancelPrompt(PromptCancellation),
    /// Correlated terminal cancellation.
    CancelTerminal(TerminalCancellation),
    /// Subscription pause or resume.
    Subscription(SubscriptionControl),
    /// Heartbeat nonce/sequence reply.
    HeartbeatReply(HeartbeatReply),
}

/// Complete typed application control frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlEnvelope {
    context: ProtocolContext,
    correlation_id: CorrelationId,
    payload: ControlPayload,
}

impl ControlEnvelope {
    /// Creates a typed control bound to one negotiated context and correlation.
    #[must_use]
    pub const fn new(
        context: ProtocolContext,
        correlation_id: CorrelationId,
        payload: ControlPayload,
    ) -> Self {
        Self { context, correlation_id, payload }
    }
    /// Returns the negotiated context.
    #[must_use]
    pub const fn context(&self) -> ProtocolContext {
        self.context
    }
    /// Returns the correlation identity.
    #[must_use]
    pub const fn correlation_id(&self) -> CorrelationId {
        self.correlation_id
    }
    /// Borrows the closed control payload.
    #[must_use]
    pub const fn payload(&self) -> &ControlPayload {
        &self.payload
    }
}
