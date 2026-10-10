//! Bounded, content-based detection of an unchanged inspection cycle.

use std::collections::{BTreeMap, VecDeque};

use peritus_model_protocol::{ContentBlock, Message};
use peritus_types::Sha256Digest;
use serde_json::Value;

const RECENT_OBSERVATIONS: usize = 16;
const WARN_REPEATS: u8 = 1;

#[derive(Default)]
pub(super) struct InspectionProgress {
    recent: VecDeque<Sha256Digest>,
    visible_observations: Option<Vec<Sha256Digest>>,
    repeats: u8,
    warning_pending: bool,
}

impl InspectionProgress {
    pub(super) fn observe_model_context(&mut self, messages: &[Message]) {
        let mut pending = BTreeMap::new();
        let mut visible = Vec::new();
        for block in messages.iter().flat_map(Message::content) {
            match block {
                ContentBlock::ToolCall(call) if is_inspection(call.name().as_str()) => {
                    if let Ok(arguments) =
                        serde_json::from_slice::<Value>(call.arguments().canonical_bytes())
                    {
                        pending
                            .insert(call.id().expose_for_wire(), (call.name().as_str(), arguments));
                    }
                }
                ContentBlock::ToolResult(result) => {
                    if let Some((name, arguments)) =
                        pending.remove(result.call_id().expose_for_wire())
                        && let Ok(mut output) =
                            serde_json::from_slice::<Value>(result.output().canonical_bytes())
                    {
                        // The host adds this source reference after producing the raw observation.
                        if let Some(object) = output.as_object_mut() {
                            object.remove("local_context");
                        }
                        visible.push(fingerprint(name, &arguments, &output));
                    }
                }
                _ => {}
            }
        }
        for digest in &visible {
            if self.recent.contains(digest) {
                continue;
            }
            if self.recent.len() == RECENT_OBSERVATIONS {
                self.recent.pop_front();
            }
            self.recent.push_back(*digest);
        }
        self.visible_observations = Some(visible);
    }

    pub(super) fn observe(
        &mut self,
        name: &str,
        arguments: &Value,
        result: &Value,
        mutation_boundary: bool,
    ) {
        if !is_inspection(name) {
            // Only a capability that may have changed the workspace invalidates content-bound
            // observations. Read-only status and polling tools cannot make the same bytes fresh.
            if mutation_boundary {
                self.recent.clear();
                self.repeats = 0;
                self.warning_pending = false;
            }
            return;
        }
        let digest = fingerprint(name, arguments, result);
        if self.recent.contains(&digest) {
            let visible = self
                .visible_observations
                .as_ref()
                .is_none_or(|observations| observations.contains(&digest));
            if visible {
                self.repeats = self.repeats.saturating_add(1);
                if self.repeats == WARN_REPEATS {
                    self.warning_pending = true;
                }
            } else {
                // Identical bytes can restore evidence that the active model view evicted.
                self.repeats = 0;
                self.warning_pending = false;
            }
        } else {
            self.repeats = 0;
            self.warning_pending = false;
            if self.recent.len() == RECENT_OBSERVATIONS {
                self.recent.pop_front();
            }
            self.recent.push_back(digest);
        }
    }

    pub(super) fn feedback(&mut self) -> Option<String> {
        std::mem::take(&mut self.warning_pending).then(|| {
            "The host detected repeated identical workspace inspection arguments AND results with no new evidence. Initial grounding is not reset between provider steps. Use the observations already obtained and take a different task-relevant step, or identify the specific missing evidence.".to_owned()
        })
    }
}

fn is_inspection(name: &str) -> bool {
    matches!(name, "workspace_list" | "workspace_read" | "workspace_search" | "attachment_read")
}

fn fingerprint(name: &str, arguments: &Value, result: &Value) -> Sha256Digest {
    peritus_codec::sha256(format!("{name}\n{arguments}\n{result}").as_bytes())
}
