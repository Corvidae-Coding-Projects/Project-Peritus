//! Object-safe effect and owned active-execution boundaries.

use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    CancellationReason, ImplementationIdentity, PreparedToolCall, ProtocolError, ResultStatus,
    SchemaDigest, ToolControl, ToolFailure, ToolProgress, ToolResult,
};

use crate::AuthorizedInvocation;

/// Stable dispatcher/lower-boundary failure ready for truthful normalization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DispatchFailure {
    status: ResultStatus,
    failure: ToolFailure,
}

impl DispatchFailure {
    /// Creates a non-success dispatch failure.
    ///
    /// # Errors
    ///
    /// Rejects `Succeeded`, which cannot carry a failure.
    pub fn new(status: ResultStatus, failure: ToolFailure) -> Result<Self, ProtocolError> {
        if status == ResultStatus::Succeeded {
            let error = ProtocolError::invalid_envelope(
                "dispatch_failure.status".to_owned(),
                "dispatch failure cannot claim success",
            );
            return Err(match error {
                Ok(error) | Err(error) => error,
            });
        }
        Ok(Self { status, failure })
    }

    /// Returns the terminal non-success status.
    #[must_use]
    pub const fn status(&self) -> ResultStatus {
        self.status
    }
    /// Borrows the exact typed failure.
    #[must_use]
    pub const fn failure(&self) -> &ToolFailure {
        &self.failure
    }
}

/// One ordered active-execution observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionUpdate {
    progress: Vec<ToolProgress>,
    terminal: Option<ToolResult>,
}

impl ExecutionUpdate {
    /// Creates an update bound to one exact prepared call.
    ///
    /// # Errors
    ///
    /// Rejects wrong identities, unordered sequences, or excess progress.
    pub fn new(
        prepared: &PreparedToolCall,
        progress: Vec<ToolProgress>,
        terminal: Option<ToolResult>,
    ) -> Result<Self, ProtocolError> {
        let action_id = prepared.call().action_id();
        let digest = prepared.prepared_digest();
        if progress.len() > prepared.call().limits().progress_events() as usize
            || progress
                .iter()
                .any(|event| event.action_id() != action_id || event.prepared_digest() != digest)
            || progress
                .windows(2)
                .any(|pair| pair[0].sequence().checked_add(1) != Some(pair[1].sequence()))
            || terminal.as_ref().is_some_and(|result| {
                result.action_id() != action_id
                    || result.prepared_digest() != digest
                    || result.replay_identity() != prepared.replay_identity()
            })
        {
            let error = ProtocolError::invalid_envelope(
                "execution_update".to_owned(),
                "execution update is unordered, over-limit, or bound to another call",
            );
            return Err(match error {
                Ok(error) | Err(error) => error,
            });
        }
        Ok(Self { progress, terminal })
    }

    /// Borrows ordered progress events.
    #[must_use]
    pub fn progress(&self) -> &[ToolProgress] {
        &self.progress
    }
    /// Borrows an optional terminal result.
    #[must_use]
    pub const fn terminal(&self) -> Option<&ToolResult> {
        self.terminal.as_ref()
    }
}

/// Recovery observation from an owned active execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryObservation {
    /// Execution remains owned and active.
    Active(ExecutionUpdate),
    /// Ordered progress and an exact terminal result were recovered together.
    Completed(ExecutionUpdate),
    /// Outcome cannot be established safely.
    Lost(DispatchFailure),
}

/// Owned active execution controlled only through the router.
pub trait ToolExecution: Send {
    /// Polls ordered progress and optional terminal state.
    ///
    /// # Errors
    ///
    /// Returns a typed lower-boundary failure for router normalization.
    fn poll(&mut self, observed_at: AuthorityInstant) -> Result<ExecutionUpdate, DispatchFailure>;

    /// Applies one supported non-cancellation control.
    ///
    /// # Errors
    ///
    /// Returns a typed lower-boundary failure for router normalization.
    fn control(
        &mut self,
        control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure>;

    /// Requests owned cancellation and observes resulting state.
    ///
    /// # Errors
    ///
    /// Returns a typed lower-boundary failure when cancellation cannot be observed safely.
    fn cancel(
        &mut self,
        reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure>;

    /// Reconciles the invocation after lost caller/daemon observation.
    ///
    /// # Errors
    ///
    /// Returns a typed lower-boundary failure when recovery itself cannot be observed.
    fn recover(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<RecoveryObservation, DispatchFailure>;
}

/// Result of the dispatcher's only effectful method.
#[allow(
    clippy::large_enum_variant,
    reason = "the published dispatcher seam returns terminal results directly and active owners indirectly"
)]
pub enum ToolStart {
    /// Invocation completed synchronously.
    Completed(ToolResult),
    /// Invocation remains owned and controllable by the router.
    Active(Box<dyn ToolExecution>),
}

/// Object-safe exact-identity implementation boundary.
pub trait ToolDispatcher {
    /// Borrows the immutable implementation/catalog identity.
    fn implementation_identity(&self) -> &ImplementationIdentity;
    /// Returns the exact descriptor digest this implementation serves.
    fn descriptor_digest(&self) -> SchemaDigest;
    /// Starts the only effect using a router-constructed move-only permit.
    ///
    /// # Errors
    ///
    /// Returns a typed failure which the router closes into a terminal result.
    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure>;
}
