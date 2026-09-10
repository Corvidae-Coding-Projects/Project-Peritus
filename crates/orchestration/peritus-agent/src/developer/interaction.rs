//! Optional live user-input and public activity boundary for developer execution.

use super::DeveloperLoopError;
pub use crate::developer_interaction::{DeveloperInput, DeveloperRequestAdmission};

/// Host-owned role whose model is selected at each new logical model turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperModelRole {
    /// Conversation, design, and implementation model.
    Writer,
    /// Independent review model.
    Reviewer,
    /// Review remediation model.
    Fixer,
}

/// Host decision after an already-admitted provider or tool operation settles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperControlFlow {
    /// The next operation may still be considered under current host control.
    Continue,
    /// Return at this safe boundary without admitting another operation.
    Stop,
}

/// Conservative effect class used for before-edit goal pauses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperToolEffect {
    /// Inspection proven not to mutate workspace or external state.
    ReadOnly,
    /// Any operation that can mutate state or whose effect class is not proven read-only.
    MutationCapable,
}

/// Public execution activity, separate from the raw durable provider trace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperActivity<'a> {
    /// A model request is about to start.
    ModelStarted {
        /// Exact requested model identifier.
        model: &'a str,
        /// Reasoning control in the outgoing request, not an inferred provider outcome.
        reasoning: peritus_model_protocol::ReasoningPolicy,
    },
    /// No public text has arrived while a provider request remains pending.
    ModelWaiting { elapsed_seconds: u64 },
    /// A bounded syntax repair was durably recorded; no response contents are exposed here.
    ResponseHealed,
    /// Public assistant text received from a provider, never a reasoning delta.
    Text(&'a [u8]),
    /// One exact tool call is about to execute.
    ToolStarted { name: &'a str, arguments: &'a str },
    /// A tool completed with a bounded, user-visible result.
    ToolFinished { name: &'a str, output: &'a str, is_error: bool },
    /// A tool call was not executed because newer user input superseded it.
    ToolSkipped { name: &'a str },
}

/// Daemon-owned live input and observation port; it cannot grant tool authority.
pub trait DeveloperInteraction: Send + Sync {
    /// Whether this host explicitly permits legacy provider-authored automatic compaction.
    /// Governed workbench hosts disable it; deterministic local preparation remains available.
    fn allows_semantic_compaction(&self) -> bool {
        true
    }
    /// Resolves an immutable adapter for the next turn, without changing an in-flight request.
    /// Returning `None` retains the caller's fixed provider.
    ///
    /// # Errors
    /// Fails closed if the selected adapter cannot be resolved.
    fn provider(
        &self,
        _role: DeveloperModelRole,
    ) -> Result<Option<std::sync::Arc<dyn peritus_provider_core::ModelProvider>>, DeveloperLoopError>
    {
        Ok(None)
    }

    /// Atomically captures current input. Failure stops rather than using a stale snapshot.
    ///
    /// # Errors
    /// Returns a durable-input or synchronization failure.
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError>;

    /// Atomically checks current input and durably binds it to the exact prepared request.
    /// Upgraded hosts publish the immutable input IDs/revisions and request fingerprint in
    /// the same transaction as incorporation. This method must not start provider execution.
    ///
    /// # Errors
    /// Returns a persistence failure; no request is sent after a failed acknowledgement.
    fn prepare_request(
        &self,
        revision: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError>;

    /// Role-aware request admission. Existing hosts retain their prior behavior by default.
    /// Goal-aware hosts override this to reserve cumulative budget before provider dispatch.
    ///
    /// # Errors
    /// Returns a durable-input, control, or synchronization failure.
    fn prepare_role_request(
        &self,
        _role: DeveloperModelRole,
        revision: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        self.prepare_request(revision, request)
    }

    /// Reconciles one admitted request at its terminal provider boundary.
    ///
    /// # Errors
    /// Returns a durable accounting or synchronization failure.
    fn complete_role_request(
        &self,
        _role: DeveloperModelRole,
        _request_id: &str,
        _usage: peritus_model_protocol::UsageCounters,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        Ok(DeveloperControlFlow::Continue)
    }

    /// Reserves one tool call before the executor can observe or perform its effect. `invocation`
    /// is the stable request prefix for this complete developer-loop invocation; `sequence` is
    /// scoped to it and may restart in a later invocation.
    ///
    /// # Errors
    /// Returns a durable control or synchronization failure.
    fn admit_tool(
        &self,
        _role: DeveloperModelRole,
        _invocation: &str,
        _sequence: u32,
        _effect: DeveloperToolEffect,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        Ok(DeveloperControlFlow::Continue)
    }

    /// Reconciles one admitted tool at the next safe boundary using the same invocation identity
    /// and loop-local sequence supplied at admission.
    ///
    /// # Errors
    /// Returns a durable accounting or synchronization failure.
    fn complete_tool(
        &self,
        _role: DeveloperModelRole,
        _invocation: &str,
        _sequence: u32,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        Ok(DeveloperControlFlow::Continue)
    }

    /// Records safe public activity independently of raw trace storage.
    ///
    /// # Errors
    /// Returns an observation failure rather than claiming invisible successful progress.
    fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError>;
}
