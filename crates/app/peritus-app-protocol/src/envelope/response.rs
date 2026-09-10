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
    /// Durable checkpoint publication with exact visible coverage and exclusions.
    WorkbenchCheckpoint(crate::WorkbenchCheckpointReceipt),
    /// Non-mutating exact restore plan awaiting confirmation.
    WorkbenchRewindPreview(crate::WorkbenchRewindPreview),
    /// Durable terminal restore receipt.
    WorkbenchRestore(crate::WorkbenchRestoreReceipt),
    /// Bounded revision-fenced project guidance and optional content-free tombstones.
    WorkbenchMemory(crate::WorkbenchMemory),
    /// Exact read-only initialization observations, patch diff, and unverified command inventory.
    InitProposal(crate::InitProposal),
    /// Effective host-intersected permission inspection with provenance.
    WorkbenchPermissions(crate::WorkbenchPermissions),
    /// Exact local deterministic prompt-view proposal awaiting explicit confirmation.
    WorkbenchCompactionPreview(crate::WorkbenchCompactionPreview),
    /// Bounded local literal-search results with exact public source snippets.
    ConversationLibrary(crate::ConversationLibraryPage),
    /// Current persistent goal, evidence, safe-boundary state, and cumulative accounting.
    WorkbenchGoal(crate::WorkbenchGoalSnapshot),
    /// Result viewer with independent launch, capture, behavior and human-review evidence.
    WorkbenchResult(crate::WorkbenchResultPage),
    /// Structured candidate diff, anchored feedback, and mapped qualification evidence.
    WorkbenchReview(crate::WorkbenchReviewPage),
    /// Revision-fenced retained image metadata and selection; no image bytes or inference.
    WorkbenchImages(crate::WorkbenchImagePage),
    /// Exact provider-bound image preview, without import acceptance or provider delivery.
    WorkbenchImagePreview(crate::WorkbenchImagePreview),
    /// Exact source/provider confirmation preview, without implicit inclusion.
    WorkbenchFilePreview(crate::WorkbenchFilePreview),
    /// Provider-bound immutable imported-text preview; source paths grant no authority.
    WorkbenchFileImportPreview(crate::WorkbenchFileImportPreview),
    /// Revision-fenced retained file versions and selection.
    WorkbenchFiles(crate::WorkbenchFilePage),
    /// Exact user-confirmed fields and immutable input provenance.
    WorkbenchBrief(crate::WorkbenchBrief),
    /// Exact content-free source manifest or eligible-input metadata page.
    WorkbenchContext(crate::WorkbenchContextPage),
    /// Exact bounded page of pending or historical immutable inputs.
    WorkbenchQueue(crate::WorkbenchQueuePage),
    /// Current revisioned durable conversation metadata.
    Workbench(crate::WorkbenchSnapshot),
    /// Durable original acceptance receipt, distinct from socket acknowledgement.
    WorkbenchReceipt(crate::WorkbenchReceipt),
    /// Scoped read-only diagnostic findings.
    Doctor(crate::DoctorReport),
    /// Conversation status and public activity.
    Interaction(crate::ProductInteractionSnapshot),
    /// Provider-discovered catalog, including explicit unavailable/cache metadata.
    Models(crate::ProductModelCatalog),
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
