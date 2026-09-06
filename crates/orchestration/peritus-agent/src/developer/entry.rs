//! Public developer-loop entry points and context policy selection.

use peritus_provider_core::ModelProvider;

use super::context_port::ContextSession;
use super::{
    DeveloperContextPort, DeveloperLoopError, DeveloperLoopOutcome, DeveloperLoopRequest,
    DeveloperToolExecutor, DeveloperTrace,
};

/// Production D0 composition that repeatedly lets a model inspect, edit, execute, and observe.
pub struct DeveloperLoop;

impl DeveloperLoop {
    /// Runs with host-owned local context retained across invocation outcomes.
    ///
    /// This path bypasses both legacy compaction policies. The port commits observations and
    /// replacement views before use; failure stops the loop without provider compaction fallback.
    /// The caller supplies a fresh tool executor to enforce each invocation's grounding rules.
    ///
    /// # Errors
    /// Returns context, protocol, provider, trace, tool, cancellation, or bound failures.
    pub async fn run_with_context(
        provider: &dyn ModelProvider,
        request: DeveloperLoopRequest,
        tools: &mut dyn DeveloperToolExecutor,
        trace: &mut dyn DeveloperTrace,
        context: &mut dyn DeveloperContextPort,
    ) -> Result<DeveloperLoopOutcome, DeveloperLoopError> {
        Self::run_inner(provider, request, tools, trace, ContextSession(Some(context))).await
    }

    /// Runs a bounded developer loop until the provider returns final text without tool calls.
    ///
    /// Provider envelopes and tool observations are recorded through `trace` before they are
    /// admitted to subsequent model context. Uses the legacy context policy.
    ///
    /// # Errors
    /// Returns typed protocol, provider, trace, tool, cancellation, or bound failures.
    pub async fn run(
        provider: &dyn ModelProvider,
        request: DeveloperLoopRequest,
        tools: &mut dyn DeveloperToolExecutor,
        trace: &mut dyn DeveloperTrace,
    ) -> Result<DeveloperLoopOutcome, DeveloperLoopError> {
        Self::run_inner(provider, request, tools, trace, ContextSession(None)).await
    }
}
