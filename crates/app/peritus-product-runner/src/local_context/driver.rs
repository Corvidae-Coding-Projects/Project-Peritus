//! Shared production invocation switch; legacy mode is explicit and never an error fallback.

use super::{LocalContextHandle, MemoryTools, memory_tool_definitions};
use crate::{budget::RunAccounting, trace::accounting::AccountingTrace};
use peritus_agent::{
    DeveloperLoop, DeveloperLoopError, DeveloperLoopOutcome, DeveloperLoopRequest,
    DeveloperToolExecutor,
};
use peritus_provider_core::ModelProvider;
use std::path::Path;

pub struct InvocationAccounting<'a> {
    pub trace_path: &'a Path,
    pub accounting: &'a mut RunAccounting,
}

pub async fn run_live_invocation(
    model: &dyn ModelProvider,
    mut request: DeveloperLoopRequest,
    tools: &mut dyn DeveloperToolExecutor,
    accounting: InvocationAccounting<'_>,
    memory: Option<&LocalContextHandle>,
    interaction: Option<&dyn peritus_agent::DeveloperInteraction>,
    role: peritus_agent::DeveloperModelRole,
) -> Result<DeveloperLoopOutcome, DeveloperLoopError> {
    if interaction.is_some() {
        request.system.push_str(LIVE_COMMUNICATION);
    }
    let mut trace = AccountingTrace::new(accounting.trace_path, accounting.accounting);
    match memory {
        Some(memory) => {
            request.tools.extend(memory_tool_definitions()?);
            trace.trace =
                trace.trace.with_memory_scope(memory.scope_digest()?, memory.next_invocation()?);
            let mut port = memory.clone();
            let mut tools = MemoryTools::new(tools, memory.clone());
            match interaction {
                Some(interaction) => {
                    DeveloperLoop::run_interactive_for_role(
                        model,
                        request,
                        &mut tools,
                        &mut trace,
                        Some(&mut port),
                        interaction,
                        role,
                    )
                    .await
                }
                None => {
                    DeveloperLoop::run_with_context(
                        model, request, &mut tools, &mut trace, &mut port,
                    )
                    .await
                }
            }
        }
        None => match interaction {
            Some(interaction) => {
                DeveloperLoop::run_interactive_for_role(
                    model,
                    request,
                    tools,
                    &mut trace,
                    None,
                    interaction,
                    role,
                )
                .await
            }
            None => DeveloperLoop::run(model, request, tools, &mut trace).await,
        },
    }
}

// Intermediate prose belongs to the same tool-bearing turn, not a separate terminal answer.
// This keeps the design/reviewer/writer terminal contracts and host-owned tool policy intact.
const LIVE_COMMUNICATION: &str = "\n\nLIVE CONVERSATION: The user is watching this work in a conversation. Before your first tool call, include a brief, ordinary-language message explaining the next step. On subsequent tool-bearing turns, share concise updates when you learn something important, change direction, begin edits, or start verification. Put this public prose alongside the host tool calls in that response (in content when using a structured host-tool envelope); do not end the turn just to announce future work. Explain what you are checking and why it matters to the user's request, rather than listing tool names. Distinguish plans, observed results, failures, and remaining uncertainty. Do not claim a tool has run before its result arrives. Do not expose private reasoning, credentials, raw tool arguments, or internal protocol envelopes. Keep routine updates to one or two sentences and avoid repeating unchanged status. For a simple question, answer directly without an unnecessary preamble. The final response must still follow the requested terminal format; these updates do not change that contract or authorize additional work.";
