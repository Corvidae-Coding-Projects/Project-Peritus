//! Closed typed application event envelope.

use crate::{
    AppDiagnostic, ArtifactChunk, ArtifactMetadata, DaemonHeartbeat, Delivery, EventCursor,
    PromptBinding, ShutdownComplete, ShutdownProgress, SubscriptionGap, SubscriptionId,
    TerminalExit, TerminalOutput, TransferId,
};
use peritus_types::{ArtifactId, Sha256Digest};

use super::ProtocolContext;

/// Explicit subscription backpressure observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SubscriptionBackpressure {
    subscription_id: SubscriptionId,
    last_delivered: EventCursor,
    last_acknowledged: EventCursor,
    maximum_in_flight: u32,
}

impl SubscriptionBackpressure {
    /// Creates a complete backpressure observation.
    #[must_use]
    pub const fn new(
        subscription_id: SubscriptionId,
        last_delivered: EventCursor,
        last_acknowledged: EventCursor,
        maximum_in_flight: u32,
    ) -> Self {
        Self { subscription_id, last_delivered, last_acknowledged, maximum_in_flight }
    }
    /// Returns the subscription identity.
    #[must_use]
    pub const fn subscription_id(self) -> SubscriptionId {
        self.subscription_id
    }
    /// Returns the last delivered cursor.
    #[must_use]
    pub const fn last_delivered(self) -> EventCursor {
        self.last_delivered
    }
    /// Returns the last acknowledged cursor.
    #[must_use]
    pub const fn last_acknowledged(self) -> EventCursor {
        self.last_acknowledged
    }
    /// Returns the full negotiated in-flight window.
    #[must_use]
    pub const fn maximum_in_flight(self) -> u32 {
        self.maximum_in_flight
    }
}

/// Exact artifact-transfer completion observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactCompletion {
    transfer_id: TransferId,
    artifact_id: ArtifactId,
    byte_size: u64,
    digest: Sha256Digest,
}

impl ArtifactCompletion {
    /// Creates exact completion metadata without claiming persistence.
    #[must_use]
    pub const fn new(
        transfer_id: TransferId,
        artifact_id: ArtifactId,
        byte_size: u64,
        digest: Sha256Digest,
    ) -> Self {
        Self { transfer_id, artifact_id, byte_size, digest }
    }
    /// Returns the transfer identity.
    #[must_use]
    pub const fn transfer_id(self) -> TransferId {
        self.transfer_id
    }
    /// Returns the artifact identity.
    #[must_use]
    pub const fn artifact_id(self) -> ArtifactId {
        self.artifact_id
    }
    /// Returns the exact completed size.
    #[must_use]
    pub const fn byte_size(self) -> u64 {
        self.byte_size
    }
    /// Returns the observed final digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
}

/// Closed schema-v1 application event payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppEventPayload {
    /// At-least-once delivery of one exact registered B3 event frame.
    DomainEvent(Delivery),
    /// Retention gap requiring snapshot/resubscription recovery.
    SubscriptionGap {
        /// Subscription whose requested cursor fell outside retention.
        subscription_id: SubscriptionId,
        /// Exact unavailable and retained cursor bounds.
        gap: SubscriptionGap,
    },
    /// Explicit in-flight delivery backpressure.
    Backpressure(SubscriptionBackpressure),
    /// Artifact transfer metadata.
    ArtifactMetadata(ArtifactMetadata),
    /// One contiguous artifact chunk.
    ArtifactChunk(ArtifactChunk),
    /// Exact transfer completion observation.
    ArtifactComplete(ArtifactCompletion),
    /// Approval or user-input prompt request.
    PromptRequested(PromptBinding),
    /// One ordered terminal output chunk.
    TerminalOutput(TerminalOutput),
    /// One final terminal exit observation.
    TerminalExited(TerminalExit),
    /// Readiness or read-only diagnostic change.
    ReadinessChanged(crate::DaemonStatus),
    /// Bounded inert diagnostic event.
    Diagnostic(AppDiagnostic),
    /// Monotonic heartbeat observation.
    Heartbeat(DaemonHeartbeat),
    /// Bounded graceful-shutdown progress.
    ShutdownProgress(ShutdownProgress),
    /// Truthful clean/unclean shutdown completion.
    ShutdownComplete(ShutdownComplete),
}

/// Complete typed application event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppEventEnvelope {
    context: ProtocolContext,
    payload: AppEventPayload,
}

impl AppEventEnvelope {
    /// Creates a typed event bound to one negotiated context.
    #[must_use]
    pub const fn new(context: ProtocolContext, payload: AppEventPayload) -> Self {
        Self { context, payload }
    }
    /// Returns the negotiated context.
    #[must_use]
    pub const fn context(&self) -> ProtocolContext {
        self.context
    }
    /// Borrows the closed event payload.
    #[must_use]
    pub const fn payload(&self) -> &AppEventPayload {
        &self.payload
    }
}
