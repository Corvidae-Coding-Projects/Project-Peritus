//! Host-owned tool execution, prerequisite, and liveness boundaries.

use peritus_model_protocol::{CompletedToolCall, Message};

use super::{DeveloperLoopError, DeveloperToolEffect, DeveloperToolObservation};

/// Executes already parsed provider tool calls against one explicitly supplied workspace.
pub trait DeveloperToolExecutor: Send {
    /// Supplies host-owned conditional guidance before context assembly and budget admission.
    /// This replaces the current policy each turn; it never accumulates in optional history.
    fn system_guidance(&mut self) -> Option<String> {
        None
    }

    /// Observes the exact model-visible messages after context assembly and protocol validation.
    ///
    /// This runs for each developer request, including retries, without changing its messages.
    /// Executors may distinguish repeated visible evidence from evidence evicted by compaction.
    ///
    /// # Errors
    /// Returns an executor bookkeeping failure before role admission or provider dispatch.
    fn observe_model_context(&mut self, _messages: &[Message]) -> Result<(), DeveloperLoopError> {
        Ok(())
    }

    /// Conservatively classifies a call before any executor-owned effect preparation.
    #[must_use]
    fn effect(&self, _call: &CompletedToolCall) -> DeveloperToolEffect {
        DeveloperToolEffect::MutationCapable
    }

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

    /// Revalidates a host-archived workspace read before reusing its evidence or coverage.
    ///
    /// Implementations must apply the same current permissions, argument/path checks and
    /// grounding bookkeeping as a normal read, and read fresh bytes if its version changed.
    /// The default declines reuse; the caller then executes the original call normally.
    ///
    /// # Errors
    /// Returns a structural dispatch failure, as for [`Self::execute`].
    fn reuse_workspace_read(
        &mut self,
        _call: &CompletedToolCall,
        _observation: &DeveloperToolObservation,
    ) -> Result<Option<DeveloperToolObservation>, DeveloperLoopError> {
        Ok(None)
    }

    /// Returns actual image observations associated with the completed tool result.
    ///
    /// Implementations must authenticate and bound the bytes before supplying them. The loop
    /// retains these in its provider-neutral transcript rather than rendering base64 as text.
    ///
    /// # Errors
    ///
    /// Returns an error when image evidence cannot be authenticated, bounded, or retained.
    fn observation_images(
        &mut self,
        _call: &CompletedToolCall,
        _observation: &DeveloperToolObservation,
    ) -> Result<Vec<peritus_model_protocol::MediaInput>, DeveloperLoopError> {
        Ok(Vec::new())
    }

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

    /// Whether this phase can request any further tools.
    ///
    /// A host-owned handoff may close inspection after its prerequisite succeeds. The loop
    /// then advertises no tools and requests a terminal response, including after a retry.
    /// This is a phase boundary, not a token, time, or execution budget.
    fn tool_calls_allowed(&self) -> bool {
        true
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
    /// The loop reads this before draining progress feedback, so a pending warning can defer
    /// stopping until the provider has received the warning and returned another tool batch.
    fn continuation_blocker(&self) -> Option<String> {
        None
    }
}
