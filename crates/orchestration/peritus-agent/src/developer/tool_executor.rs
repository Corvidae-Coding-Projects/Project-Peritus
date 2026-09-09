//! Host-owned tool execution, prerequisite, and liveness boundaries.

use peritus_model_protocol::CompletedToolCall;

use super::{DeveloperLoopError, DeveloperToolObservation};

/// Executes already parsed provider tool calls against one explicitly supplied workspace.
pub trait DeveloperToolExecutor: Send {
    /// Whether an executed tool handed control back to its owning application.
    ///
    /// The loop records the observation, skips remaining calls in the batch, and returns its
    /// accounting without another model turn. This is host state, never a provider verdict.
    fn yields_to_host(&self) -> bool {
        false
    }

    /// Executes one call and returns a model-safe observation.
    ///
    /// # Errors
    /// Returns a structural dispatch failure. Ordinary command failures should be represented as
    /// `DeveloperToolObservation { is_error: true, .. }` so the model can inspect and retry.
    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError>;

    /// Explains why a text-only model response cannot yet complete this tool session.
    ///
    /// The developer loop feeds this reason back into the same conversation and keeps the
    /// executor alive, preserving partial inspection evidence across the correction. Executors
    /// without a completion precondition use the default ready state.
    fn completion_blocker(&self) -> Option<String> {
        None
    }

    /// Names the one declared tool that must run before other work can make progress.
    ///
    /// The developer loop projects this as a provider-level specific tool choice. This lets an
    /// executor enforce deterministic prerequisites such as repository grounding without relying
    /// on a model to infer the next protocol step from corrective prose.
    fn required_tool_name(&self) -> Option<&str> {
        None
    }

    /// Returns and clears a deterministic correction after an unproductive tool sequence.
    ///
    /// The developer loop appends this as a user message after the current tool batch. Executors
    /// without application-specific progress evidence use the default no-feedback behavior.
    fn take_progress_feedback(&mut self) -> Option<String> {
        None
    }

    /// Explains a terminal no-progress condition after a complete tool batch is retained.
    ///
    /// Unlike a completion prerequisite, this ends the invocation with a nonretryable tool
    /// failure. It must not be used for ordinary command failures or expected polling states.
    fn continuation_blocker(&self) -> Option<String> {
        None
    }
}
