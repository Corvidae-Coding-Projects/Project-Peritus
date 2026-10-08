//! Object-safe effect and owned active-execution boundaries.

use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    BoundedText, CancellationReason, FailureCategory, ImplementationIdentity, PreparedToolCall,
    ProtocolError, ResponsibleSubsystem, ResultStatus, RecoveryRoute, Retryability, SchemaDigest,
    ToolControl, ToolFailure, ToolProgress, ToolResult,
};

use crate::AuthorizedInvocation;

/// Retry guidance for a control proven not to have entered the execution owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlRetryability {
    /// Retry the same control on the same invocation when queue capacity is available.
    WhenReady,
    /// Correct the control while preserving the same invocation.
    CorrectRequest,
    /// Observe the retained invocation; its control admission is closed.
    ObserveOnly,
}

/// Stable lower-boundary failure, distinct from an execution's terminal observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DispatchFailure {
    status: ResultStatus,
    failure: ToolFailure,
    control_retryability: Option<ControlRetryability>,
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
        Ok(Self { status, failure, control_retryability: None })
    }

    /// Marks a control failure which occurred before admission to the execution owner.
    ///
    /// This must not be used for an observation failure following an accepted stdin write,
    /// signal, resize, or cancellation. Repeating an admitted effect is not a safe retry.
    #[must_use]
    pub const fn rejecting_control(mut self, retryability: ControlRetryability) -> Self {
        self.control_retryability = Some(retryability);
        self
    }

    /// Returns the non-success status used when normalizing a dispatch-start failure.
    ///
    /// An active-operation error does not itself establish execution terminal truth.
    #[must_use]
    pub const fn status(&self) -> ResultStatus {
        self.status
    }
    /// Borrows the exact typed failure.
    #[must_use]
    pub const fn failure(&self) -> &ToolFailure {
        &self.failure
    }

    /// Returns retry guidance for a control explicitly rejected before admission.
    #[must_use]
    pub const fn control_retryability(&self) -> Option<ControlRetryability> {
        self.control_retryability
    }
}

impl core::fmt::Display for DispatchFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}: {}", self.failure.code().as_str(), self.failure.detail().as_str())
    }
}

impl std::error::Error for DispatchFailure {}

/// One ordered active-execution observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionUpdate {
    progress: Vec<ToolProgress>,
    progress_page: Option<crate::ProgressPage>,
    terminal: Option<ToolResult>,
    settlement_failure: Option<DispatchFailure>,
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
        Self::validated(prepared, progress, terminal, None)
    }

    /// Creates an active update whose exact terminal settlement cannot yet close and must be
    /// retried through the retained execution owner.
    ///
    /// # Errors
    ///
    /// Rejects wrong identities, unordered sequences, excess progress, or a failure which does
    /// not require reconciliation before retry.
    pub fn settlement_pending(
        prepared: &PreparedToolCall,
        progress: Vec<ToolProgress>,
        failure: DispatchFailure,
    ) -> Result<Self, ProtocolError> {
        Self::validated(prepared, progress, None, Some(failure))
    }

    fn validated(
        prepared: &PreparedToolCall,
        progress: Vec<ToolProgress>,
        terminal: Option<ToolResult>,
        settlement_failure: Option<DispatchFailure>,
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
            || terminal.is_some() && settlement_failure.is_some()
            || settlement_failure.as_ref().is_some_and(|failure| {
                failure.failure().retryability() != Retryability::AfterRecovery
                    || matches!(
                        failure.failure().recovery(),
                        RecoveryRoute::None
                            | RecoveryRoute::Reauthorize
                            | RecoveryRoute::SelectBackend
                    )
            })
        {
            let error = ProtocolError::invalid_envelope(
                "execution_update".to_owned(),
                "execution update is unordered, over-limit, bound to another call, or has invalid settlement state",
            );
            return Err(match error {
                Ok(error) | Err(error) => error,
            });
        }
        Ok(Self { progress, progress_page: None, terminal, settlement_failure })
    }

    /// Borrows ordered progress events.
    #[must_use]
    pub fn progress(&self) -> &[ToolProgress] {
        &self.progress
    }
    /// Borrows the durable page receipt attached by the router for V2 progress.
    #[must_use]
    pub const fn progress_page(&self) -> Option<&crate::ProgressPage> {
        self.progress_page.as_ref()
    }
    /// Borrows an optional terminal result.
    #[must_use]
    pub const fn terminal(&self) -> Option<&ToolResult> {
        self.terminal.as_ref()
    }
    /// Borrows a retryable failure which retains this execution as the settlement owner.
    #[must_use]
    pub const fn settlement_failure(&self) -> Option<&DispatchFailure> {
        self.settlement_failure.as_ref()
    }

    pub(crate) fn bind_progress_page(&mut self, page: Option<crate::ProgressPage>) {
        self.progress_page = page;
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
    /// Returns a typed observation failure while the router retains this execution owner.
    fn poll(&mut self, observed_at: AuthorityInstant) -> Result<ExecutionUpdate, DispatchFailure>;

    /// Applies one supported non-cancellation control.
    ///
    /// # Errors
    ///
    /// Returns a typed request or observation failure while the router retains this owner.
    /// Only a failure explicitly tagged before admission permits retrying the control.
    fn control(
        &mut self,
        control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure>;

    /// Requests owned cancellation and observes resulting state.
    ///
    /// # Errors
    ///
    /// Returns a typed failure while the router retains ownership and admitted cancel intent.
    fn cancel(
        &mut self,
        reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure>;

    /// Reconciles the invocation after lost caller/daemon observation.
    ///
    /// # Errors
    ///
    /// Returns a typed failure while the router retains ownership for further reconciliation.
    fn recover(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<RecoveryObservation, DispatchFailure>;

    /// Commits an already returned V2 progress page after the router durably accepted it.
    ///
    /// Legacy executions need no separate acknowledgement. Paged implementations must retain
    /// exactly the same page and native/parser cursor until this method succeeds.
    fn acknowledge_progress(
        &mut self,
        _next_frontier: u64,
    ) -> Result<(), DispatchFailure> {
        let code = BoundedText::new("progress-ack-unsupported".to_owned())
            .expect("static progress acknowledgement code is bounded");
        let detail = BoundedText::new(
            "paged execution did not implement durable progress acknowledgement".to_owned(),
        )
        .expect("static progress acknowledgement detail is bounded");
        Err(DispatchFailure::new(
            ResultStatus::Indeterminate,
            ToolFailure::new(
                FailureCategory::Infrastructure,
                code,
                ResponsibleSubsystem::Router,
                Retryability::AfterRecovery,
                RecoveryRoute::HumanReview,
                detail,
            ),
        )
        .expect("static progress acknowledgement failure is non-success"))
    }
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
