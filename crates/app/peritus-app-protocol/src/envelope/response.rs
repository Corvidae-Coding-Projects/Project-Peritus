//! Closed typed application response envelope.

use crate::{
    AppProtocolError, ArtifactMetadata, CommandResult, CorrelationId, EventCursor,
    ProductRunConversation, ProductRunSettlementSnapshot, ProductRunSnapshot, PromptId, RequestId,
    ShutdownAccepted, SubscriptionId, TerminalBinding,
};

use super::ProtocolContext;

/// Successful subscription-start observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SubscriptionStarted {
    subscription_id: SubscriptionId,
    after: EventCursor,
    maximum_in_flight: u32,
}

impl SubscriptionStarted {
    /// Creates a successful exact subscription-start observation.
    #[must_use]
    pub const fn new(
        subscription_id: SubscriptionId,
        after: EventCursor,
        maximum_in_flight: u32,
    ) -> Self {
        Self { subscription_id, after, maximum_in_flight }
    }
    /// Returns the established subscription identity.
    #[must_use]
    pub const fn subscription_id(self) -> SubscriptionId {
        self.subscription_id
    }
    /// Returns the cursor after which delivery starts.
    #[must_use]
    pub const fn after(self) -> EventCursor {
        self.after
    }
    /// Returns the established delivery window.
    #[must_use]
    pub const fn maximum_in_flight(self) -> u32 {
        self.maximum_in_flight
    }
}

/// Generic successful acknowledgement of one exact request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OperationAcknowledgement {
    request_id: RequestId,
}

impl OperationAcknowledgement {
    /// Creates an acknowledgement for one exact request.
    #[must_use]
    pub const fn new(request_id: RequestId) -> Self {
        Self { request_id }
    }
    /// Returns the acknowledged request.
    #[must_use]
    pub const fn request_id(self) -> RequestId {
        self.request_id
    }
}

/// Closed schema-v1 application response payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppResponsePayload {
    /// Final command status and exact committed range.
    CommandResult(CommandResult),
    /// Successful subscription establishment.
    SubscriptionStarted(SubscriptionStarted),
    /// Checked artifact metadata for an opened transfer.
    ArtifactOpened(ArtifactMetadata),
    /// A prompt answer/cancellation was accepted as protocol input, not authority.
    PromptAccepted(PromptId),
    /// Successful terminal attachment observation.
    TerminalAttached(TerminalBinding),
    /// Successful exact request acknowledgement.
    Acknowledged(OperationAcknowledgement),
    /// Current daemon status.
    DaemonStatus(crate::DaemonStatus),
    /// Explicit shutdown-request acceptance.
    ShutdownAccepted(ShutdownAccepted),
    /// Machine-actionable terminal request failure.
    Error(AppProtocolError),
    /// The run was accepted and its initial state is observable.
    ProductRunAccepted(ProductRunSnapshot),
    /// Bounded recent or exact product-run observations.
    ProductRuns(Vec<ProductRunSnapshot>),
    /// Complete bounded conversation for one exact product run.
    ProductRunConversation(ProductRunConversation),
    /// One exact product run paired with its verified terminal settlement.
    ProductRunSettled(ProductRunSettlementSnapshot),
    /// Bounded settled product-run observations.
    ProductRunSettlements(Vec<ProductRunSettlementSnapshot>),
}

/// Complete typed terminal response to one request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppResponseEnvelope {
    context: ProtocolContext,
    request_id: RequestId,
    correlation_id: CorrelationId,
    payload: AppResponsePayload,
}

impl AppResponseEnvelope {
    /// Creates a complete response that echoes the request and correlation identities.
    #[must_use]
    pub const fn new(
        context: ProtocolContext,
        request_id: RequestId,
        correlation_id: CorrelationId,
        payload: AppResponsePayload,
    ) -> Self {
        Self { context, request_id, correlation_id, payload }
    }
    /// Returns the negotiated context.
    #[must_use]
    pub const fn context(&self) -> ProtocolContext {
        self.context
    }
    /// Returns the echoed request identity.
    #[must_use]
    pub const fn request_id(&self) -> RequestId {
        self.request_id
    }
    /// Returns the echoed correlation identity.
    #[must_use]
    pub const fn correlation_id(&self) -> CorrelationId {
        self.correlation_id
    }
    /// Borrows the closed response payload.
    #[must_use]
    pub const fn payload(&self) -> &AppResponsePayload {
        &self.payload
    }
}
