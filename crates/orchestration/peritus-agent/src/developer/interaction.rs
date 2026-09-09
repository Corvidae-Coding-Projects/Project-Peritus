//! Optional live user-input and public activity boundary for developer execution.

use super::DeveloperLoopError;

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

/// One atomically captured governing conversation revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeveloperInput {
    /// Monotonic durable user-input revision.
    pub revision: u64,
    /// Current user-visible conversation, without private reasoning or credentials.
    pub conversation: String,
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

    /// Records the revision included in the next provider request.
    ///
    /// # Errors
    /// Returns a persistence failure; no request is sent after a failed acknowledgement.
    fn applied(&self, revision: u64) -> Result<(), DeveloperLoopError>;

    /// Records safe public activity independently of raw trace storage.
    ///
    /// # Errors
    /// Returns an observation failure rather than claiming invisible successful progress.
    fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError>;
}
