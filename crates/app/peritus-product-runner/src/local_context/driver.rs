//! Shared production invocation switch; legacy mode is explicit and never an error fallback.

use super::{LocalContextHandle, MemoryTools, memory_tool_definitions};
use crate::trace::FileDeveloperTrace;
use peritus_agent::{
    DeveloperLoop, DeveloperLoopError, DeveloperLoopOutcome, DeveloperLoopRequest,
    DeveloperToolExecutor,
};
use peritus_provider_core::ModelProvider;
use std::path::Path;

pub async fn run_live_invocation(
    model: &dyn ModelProvider,
    mut request: DeveloperLoopRequest,
    tools: &mut dyn DeveloperToolExecutor,
    trace_path: &Path,
    memory: Option<&LocalContextHandle>,
    interaction: Option<&dyn peritus_agent::DeveloperInteraction>,
) -> Result<DeveloperLoopOutcome, DeveloperLoopError> {
    let mut trace = FileDeveloperTrace::new(trace_path.to_path_buf());
    match memory {
        Some(memory) => {
            request.tools.extend(memory_tool_definitions()?);
            trace = trace.with_memory_scope(memory.scope_digest()?, memory.next_invocation()?);
            let mut port = memory.clone();
            let mut tools = MemoryTools::new(tools, memory.clone());
            match interaction {
                Some(interaction) => {
                    DeveloperLoop::run_interactive(
                        model,
                        request,
                        &mut tools,
                        &mut trace,
                        Some(&mut port),
                        interaction,
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
                DeveloperLoop::run_interactive(model, request, tools, &mut trace, None, interaction)
                    .await
            }
            None => DeveloperLoop::run(model, request, tools, &mut trace).await,
        },
    }
}
