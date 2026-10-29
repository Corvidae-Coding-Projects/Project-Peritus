//! Conservative decision compiler. Exact evidence is separate from native protocol history.

mod bindings;
mod compose;
mod dependencies;
mod frontier;
mod index;
mod key;
mod plan;
mod render;

use crate::local_context::{error, memory::LocalMemory};
pub(in crate::local_context) use dependencies::capture as request_dependencies;
pub(in crate::local_context) use index::Index;
use peritus_agent::{DeveloperLoopError, estimate_developer_request_tokens};
use peritus_model_protocol::{Message, ProviderProfile, Role, ToolDefinition};
use peritus_provider_core::ModelProvider;

struct Envelope<'a> {
    tools: &'a [ToolDefinition],
    profile: &'a ProviderProfile,
    provider: Option<&'a dyn ModelProvider>,
}
impl Envelope<'_> {
    fn fits(&self, messages: &[Message]) -> Result<bool, DeveloperLoopError> {
        if estimate_developer_request_tokens(messages, self.tools)
            > self.profile.limits().max_input_tokens()
        {
            return Ok(false);
        }
        self.provider.map_or(Ok(true), |provider| {
            provider
                .context_input_fits(messages, self.tools)
                .map_err(|_| error("encode decision provider envelope"))
        })
    }
}

pub(super) fn append(
    memory: &mut LocalMemory,
    messages: &mut Vec<Message>,
    selected: &mut Vec<u64>,
    tools: &[ToolDefinition],
    profile: &ProviderProfile,
    provider: Option<&dyn ModelProvider>,
) -> Result<(usize, u64), DeveloperLoopError> {
    let reference_messages = messages.clone();
    let reference_selected = selected.clone();
    let envelope = Envelope { tools, profile, provider };
    let Some(plan) = plan::Plan::prepare(memory, messages, tools)? else {
        fallback(memory, "native_replay_dependency");
        return memory.append_history(
            messages,
            selected,
            tools,
            profile.limits().max_input_tokens(),
        );
    };
    let (omitted_entries, stats) = compose::append(memory, messages, selected, &plan, &envelope)?;
    if !envelope.fits(messages)? {
        *messages = reference_messages;
        *selected = reference_selected;
        memory.read_reuse.clear_rendered();
        if plan.excluded {
            messages.push(super::text_message(Role::Developer,
                "Readable historical evidence requires paging with context_sources and its named retrieval tool. Derived/native history is withheld under current source restrictions. Pending outcomes require original-handle reconciliation, never redispatch.".to_owned())?);
            fallback(memory, "readable_evidence_paged");
            return Ok((omitted_entries, plan.uncompacted));
        }
        fallback(memory, "adapter_envelope");
        return memory.append_history(
            messages,
            selected,
            tools,
            profile.limits().max_input_tokens(),
        );
    }
    crate::efficiency::event(
        &memory.trace_path,
        serde_json::json!({"event":"decision-view","compiler_version":1,
        "indexed_sources":memory.decision_index.processed(),"distinct_evidence":plan.evidence.len(),
        "frontier_entries":plan.frontier.entries,"frontier_source_priorities":plan.frontier.sources.len(),
        "omitted_cards":stats.omitted,"partial_cards":stats.partial,"native_exchanges":plan.native_groups,
        "derived_dependencies_withheld":plan.excluded,
        "dependencies":"all distinct supplied tool evidence and whole candidate; model scopes cannot certify independence"}),
    );
    Ok((omitted_entries, plan.uncompacted))
}

fn fallback(memory: &LocalMemory, reason: &str) {
    crate::efficiency::event(
        &memory.trace_path,
        serde_json::json!({"event":"decision-view-fallback","compiler_version":1,"reason":reason}),
    );
}

pub(super) fn record_key(
    memory: &LocalMemory,
    messages: &[Message],
    profile: &ProviderProfile,
    tools: &[ToolDefinition],
) -> Result<(), DeveloperLoopError> {
    key::record(memory, messages, profile, tools)
}

pub(in crate::local_context) fn record_request(
    memory: &LocalMemory,
    request: &peritus_model_protocol::ModelRequest,
) -> Result<(), DeveloperLoopError> {
    key::record_request(memory, request)
}
