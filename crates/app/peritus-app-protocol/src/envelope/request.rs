//! Closed typed application request envelope.

use crate::{
    AppErrorCode, AppProtocolError, ArtifactCancellation, ArtifactChunk, ArtifactCompletion,
    ArtifactMetadata, CommandBinding, CorrelationId, EventCursor, PromptAnswer, PromptCancellation,
    RequestId, ShutdownRequest, SubscriptionFilter, SubscriptionId, TerminalBinding,
    TerminalCancellation, TerminalDetach, TerminalInput, TerminalResize, TransferId,
};
use peritus_types::ArtifactId;

use super::ProtocolContext;

/// Checked event-subscription creation request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionRequest {
    subscription_id: SubscriptionId,
    filter: SubscriptionFilter,
    after: EventCursor,
    maximum_in_flight: u32,
    snapshot_acceptable: bool,
}

impl SubscriptionRequest {
    /// Creates a subscription request with a positive negotiated delivery window.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::InvalidLimits`] for a zero window.
    pub fn new(
        subscription_id: SubscriptionId,
        filter: SubscriptionFilter,
        after: EventCursor,
        maximum_in_flight: u32,
        snapshot_acceptable: bool,
    ) -> Result<Self, AppProtocolError> {
        if maximum_in_flight == 0 {
            Err(AppProtocolError::new(AppErrorCode::InvalidLimits, None))
        } else {
            Ok(Self { subscription_id, filter, after, maximum_in_flight, snapshot_acceptable })
        }
    }

    /// Returns the new subscription identity.
    #[must_use]
    pub const fn subscription_id(&self) -> SubscriptionId {
        self.subscription_id
    }
    /// Borrows the canonical topic filter.
    #[must_use]
    pub const fn filter(&self) -> &SubscriptionFilter {
        &self.filter
    }
    /// Returns the cursor after which delivery begins.
    #[must_use]
    pub const fn after(&self) -> EventCursor {
        self.after
    }
    /// Returns the requested positive in-flight window.
    #[must_use]
    pub const fn maximum_in_flight(&self) -> u32 {
        self.maximum_in_flight
    }
    /// Returns whether a retained snapshot is an acceptable gap recovery.
    #[must_use]
    pub const fn snapshot_acceptable(&self) -> bool {
        self.snapshot_acceptable
    }
}

/// Request to open one exact artifact transfer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactOpenRequest {
    transfer_id: TransferId,
    artifact_id: ArtifactId,
}

impl ArtifactOpenRequest {
    /// Creates an exact artifact-open request.
    #[must_use]
    pub const fn new(transfer_id: TransferId, artifact_id: ArtifactId) -> Self {
        Self { transfer_id, artifact_id }
    }
    /// Returns the caller-selected transfer identity.
    #[must_use]
    pub const fn transfer_id(self) -> TransferId {
        self.transfer_id
    }
    /// Returns the requested artifact identity.
    #[must_use]
    pub const fn artifact_id(self) -> ArtifactId {
        self.artifact_id
    }
}

/// Closed schema-v1 application request payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppRequestPayload {
    /// Submits one exact, idempotent B3 command binding.
    SubmitCommand(CommandBinding),
    /// Starts or resumes one event subscription.
    Subscribe(SubscriptionRequest),
    /// Opens a bounded artifact transfer.
    OpenArtifact(ArtifactOpenRequest),
    /// Cancels an artifact transfer.
    CancelArtifact(ArtifactCancellation),
    /// Begins one bounded artifact upload from exact declared metadata.
    BeginArtifactUpload(ArtifactMetadata),
    /// Supplies one contiguous artifact-upload chunk.
    UploadArtifactChunk(ArtifactChunk),
    /// Completes one artifact upload with exact size and digest.
    CompleteArtifactUpload(ArtifactCompletion),
    /// Answers an approval or user-input prompt.
    AnswerPrompt(PromptAnswer),
    /// Cancels an outstanding prompt.
    CancelPrompt(PromptCancellation),
    /// Attaches to one C2-owned terminal process.
    AttachTerminal(TerminalBinding),
    /// Sends bounded terminal input.
    TerminalInput(TerminalInput),
    /// Resizes an attached terminal.
    TerminalResize(TerminalResize),
    /// Detaches an attached terminal.
    DetachTerminal(TerminalDetach),
    /// Cancels an attached terminal.
    CancelTerminal(TerminalCancellation),
    /// Requests current read-only daemon status.
    DaemonStatus,
    /// Requests graceful daemon shutdown without implying acceptance.
    Shutdown(ShutdownRequest),
}

/// Complete typed request envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppRequestEnvelope {
    context: ProtocolContext,
    request_id: RequestId,
    correlation_id: CorrelationId,
    payload: AppRequestPayload,
}

impl AppRequestEnvelope {
    /// Creates a request and checks duplicated command/shutdown correlation fields.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::CommandBindingMismatch`] when an inner binding disagrees with the
    /// outer request or correlation identity.
    pub fn new(
        context: ProtocolContext,
        request_id: RequestId,
        correlation_id: CorrelationId,
        payload: AppRequestPayload,
    ) -> Result<Self, AppProtocolError> {
        let matches = match &payload {
            AppRequestPayload::SubmitCommand(command) => {
                command.request_id() == request_id
                    && command.correlation_id() == correlation_id
                    && command.session_id() == context.session_id()
            }
            AppRequestPayload::Shutdown(shutdown) => {
                shutdown.request_id() == request_id && shutdown.correlation_id() == correlation_id
            }
            _ => true,
        };
        if !matches {
            return Err(AppProtocolError::new(AppErrorCode::CommandBindingMismatch, None));
        }
        Ok(Self { context, request_id, correlation_id, payload })
    }

    /// Returns the negotiated context.
    #[must_use]
    pub const fn context(&self) -> ProtocolContext {
        self.context
    }
    /// Returns the request identity.
    #[must_use]
    pub const fn request_id(&self) -> RequestId {
        self.request_id
    }
    /// Returns the correlation identity.
    #[must_use]
    pub const fn correlation_id(&self) -> CorrelationId {
        self.correlation_id
    }
    /// Borrows the closed request payload.
    #[must_use]
    pub const fn payload(&self) -> &AppRequestPayload {
        &self.payload
    }
}
