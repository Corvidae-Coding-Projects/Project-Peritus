//! Role-scoped repeat evidence, rebuilt from durable outcomes rather than invocation counters.
use super::{memory::LocalMemory, record::ArchiveKind};
use peritus_agent::{DeveloperLoopError, DeveloperToolObservation};
use peritus_model_protocol::{CompletedToolCall, ContentBlock, Message};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Activity {
    indexed: usize,
    seen: BTreeMap<[u8; 32], u64>,
    visible: BTreeMap<[u8; 32], Option<u64>>,
    feedback: Option<String>,
    context_reads: BTreeMap<[u8; 32], Value>,
}

pub fn inspection(name: &str) -> bool {
    matches!(
        name,
        "workspace_list"
            | "workspace_read"
            | "workspace_search"
            | "attachment_read"
            | "developer_evidence_read"
            | "skill_read"
            | "skill_list"
            | "context_read"
            | "context_sources"
            | "inspection_plan"
            | "inspection_result_read"
            | "command_output"
            | "web_search"
            | "web_open"
            | "web_read"
            | "web_find"
            | "module_attachment_read"
            | "computer_status"
            | "computer_snapshot"
            | "view_image"
    )
}

pub fn identity(name: &str, value: &Value) -> [u8; 32] {
    let mut value = value.clone();
    if let Some(object) = value.as_object_mut() {
        for key in ["local_context", "execution_resources", "base_revision"] {
            object.remove(key);
        }
    }
    peritus_codec::sha256(json!([name, value]).to_string().as_bytes()).into_bytes()
}

impl Activity {
    pub(super) fn refresh(&mut self, memory: &LocalMemory) {
        for source in &memory.sources[self.indexed..] {
            if source.kind != ArchiveKind::ToolOutput {
                continue;
            }
            let Some(call) = &source.call else { continue };
            if !inspection(&call.name) && !source.is_error {
                continue;
            }
            self.seen.insert(
                raw_key(&call.name, call.arguments_digest, source.artifact.digest.into_bytes()),
                source.sequence,
            );
            if self.seen.len() > 16384 {
                self.seen.pop_first();
            }
        }
        self.indexed = memory.sources.len();
    }

    pub(super) fn observe_view(&mut self, messages: &[Message]) {
        self.visible.clear();
        self.context_reads.clear();
        let mut calls = BTreeMap::new();
        for block in messages.iter().flat_map(Message::content) {
            match block {
                ContentBlock::ToolCall(call) => {
                    calls.insert(call.id().expose_for_wire(), call);
                }
                ContentBlock::ToolResult(result) => {
                    if let Some(call) = calls.remove(result.call_id().expose_for_wire())
                        && let Ok(value) =
                            serde_json::from_slice::<Value>(result.output().canonical_bytes())
                    {
                        let id = value["local_context"]["handle"]
                            .as_str()
                            .and_then(|handle| handle.rsplit(':').next()?.parse::<u64>().ok());
                        self.visible.insert(
                            key(
                                call.name().as_str(),
                                &value,
                                result.is_error(),
                                peritus_codec::sha256(call.arguments().canonical_bytes())
                                    .into_bytes(),
                            ),
                            id,
                        );
                        if call.name().as_str() == "context_read"
                            && !result.is_error()
                            && value.get("sources").is_some()
                            && let Ok(arguments) =
                                serde_json::from_slice::<Value>(call.arguments().canonical_bytes())
                            && arguments["observation_ids"]
                                .as_array()
                                .is_some_and(|ids| !ids.is_empty())
                            && arguments["query"].is_null()
                            && arguments["cursor"].is_null()
                        {
                            self.context_reads.insert(peritus_codec::sha256(call.arguments().canonical_bytes()).into_bytes(), json!({"observation_reuse":{"state":"already_visible","tool":"context_read","sources":arguments["observation_ids"],"offset":arguments["offset"],"instruction":"The exact immutable source page is already present in this model view. Use it and continue; reread only when that content is missing."}}));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn outcome(
        &mut self,
        memory: &LocalMemory,
        call: &CompletedToolCall,
        output: &DeveloperToolObservation,
    ) -> Result<Option<Value>, DeveloperLoopError> {
        self.refresh(memory);
        let name = call.name().as_str();
        if let Ok(value) = serde_json::from_slice::<Value>(output.output.canonical_bytes())
            && value["observation_reuse"]["state"] == "already_visible"
        {
            self.feedback = Some(format!(
                "{name} requested unchanged evidence already in this model view. Continue the unfinished task step using it; do not repeat grounding."
            ));
            return Ok(None);
        }
        if !inspection(name) && !output.is_error {
            return Ok(None);
        }
        let value: Value = serde_json::from_slice(output.output.canonical_bytes())
            .map_err(|_| super::error("decode tool outcome for reuse"))?;
        let key = key(
            name,
            &value,
            output.is_error,
            peritus_codec::sha256(call.arguments().canonical_bytes()).into_bytes(),
        );
        let reused_read = value["read_reuse"]["state"] == "already_visible";
        let arguments_digest =
            peritus_codec::sha256(call.arguments().canonical_bytes()).into_bytes();
        let output_digest = peritus_codec::sha256(output.output.canonical_bytes());
        let raw = raw_key(name, arguments_digest, output_digest.into_bytes());
        let id = self
            .seen
            .get(&raw)
            .copied()
            .or_else(|| self.visible.get(&key).copied().flatten())
            .or_else(|| {
                if self.seen.len() < 16384 {
                    return None;
                }
                memory
                    .sources
                    .iter()
                    .rev()
                    .find(|source| {
                        source.kind == ArchiveKind::ToolOutput
                            && source.artifact.digest == output_digest
                            && source.call.as_ref().is_some_and(|prior| {
                                prior.name == name && prior.arguments_digest == arguments_digest
                            })
                    })
                    .map(|source| source.sequence)
            })
            .or_else(|| reused_read.then_some(0));
        let Some(id) = id else { return Ok(None) };
        // A restored page can be useful after omission. A repeated failure never repairs its cause.
        if output.is_error || reused_read || self.visible.contains_key(&key) {
            self.feedback = Some(if output.is_error {
                format!(
                    "The {name} call repeats the same rejected outcome. Change the invalid arguments, satisfy the named permission/precondition, or continue another authorized step. Re-reading unrelated source cannot repair this rejection. No effect was established by the rejection."
                )
            } else {
                format!(
                    "The {name} result is unchanged evidence already delivered. Continue the current task from the retained plan; choose the next useful action or name the specific missing evidence. A provider retry, unrelated command, or no-op does not reset grounding."
                )
            });
            crate::efficiency::event(
                &memory.trace_path,
                json!({"event":"repeated-observation", "tool":name,"is_error":output.is_error,"source":id,"response_bytes":output.output.canonical_bytes().len()}),
            );
            // Workspace reads have exact range-aware reuse. Live queries still execute first;
            // only a byte-identical result in the actual model view gets a compact receipt.
            if !output.is_error
                && !matches!(
                    name,
                    "workspace_read" | "view_image" | "computer_snapshot" | "computer_status"
                )
                && id > 0
                && self.visible.contains_key(&key)
            {
                return Ok(Some(
                    json!({"observation_reuse":{"state":"already_visible","tool":name,"source":super::tools::source_handle(memory,id),"sha256":super::tools::hex(peritus_codec::sha256(output.output.canonical_bytes()).as_bytes()),"instruction":"Use the unchanged result already in this model view. Retrieve the exact archived source with context_read only if its content is missing; do not restart discovery."}}),
                ));
            }
        }
        Ok(None)
    }

    pub(super) fn context_candidate(&self, call: &CompletedToolCall) -> Option<Value> {
        self.context_reads
            .get(&peritus_codec::sha256(call.arguments().canonical_bytes()).into_bytes())
            .cloned()
    }

    pub(super) const fn feedback(&mut self) -> Option<String> {
        self.feedback.take()
    }
}

fn key(name: &str, value: &Value, error: bool, arguments: [u8; 32]) -> [u8; 32] {
    let _ = error;
    peritus_codec::sha256(json!([arguments, identity(name, value)]).to_string().as_bytes())
        .into_bytes()
}

fn raw_key(name: &str, arguments: [u8; 32], output: [u8; 32]) -> [u8; 32] {
    peritus_codec::sha256(json!([name, arguments, output]).to_string().as_bytes()).into_bytes()
}
