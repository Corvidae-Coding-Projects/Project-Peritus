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

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: impl Into<Value>) -> Value {
        Value::from_iter([("path", value.into())])
    }

    #[test]
    fn identical_and_alternating_inspections_warn_once() {
        for width in [1, 2] {
            let mut progress = InspectionProgress::default();
            for index in 0..width {
                progress.observe("workspace_read", &path(index), &Value::from("same"), false);
            }
            for index in 1..=32 {
                progress.observe(
                    "workspace_read",
                    &path(index % width),
                    &Value::from("same"),
                    false,
                );
                assert_eq!(progress.feedback().is_some(), index == WARN_REPEATS);
            }
            assert!(progress.feedback().is_none());
        }
    }

    #[test]
    fn new_evidence_and_noninspection_work_reset_the_cycle() {
        let mut progress = InspectionProgress::default();
        for index in 0..32 {
            progress.observe("workspace_read", &path("state"), &Value::from(index), false);
        }
        assert_eq!(progress.recent.len(), RECENT_OBSERVATIONS);
        for name in ["run_command", "command_poll", "workspace_write"] {
            for _ in 0..3 {
                progress.observe("workspace_list", &path("."), &Value::Array(Vec::new()), false);
                progress.feedback();
            }
            assert_eq!(progress.repeats, 2);
            progress.observe(
                name,
                &Value::Object(serde_json::Map::new()),
                &Value::from_iter([("success", Value::from(false))]),
                true,
            );
            assert_eq!(progress.repeats, 0);
            assert!(progress.feedback().is_none());
        }
    }

    #[test]
    fn a_repeated_batch_delivers_its_warning_once() {
        let mut progress = InspectionProgress::default();
        progress.observe("workspace_read", &path("state"), &Value::from("same"), false);
        for _ in 0..3 {
            progress.observe("workspace_read", &path("state"), &Value::from("same"), false);
        }
        assert!(progress.feedback().is_some());
        progress.observe("workspace_read", &path("state"), &Value::from("same"), false);
        assert!(progress.feedback().is_none());
    }
}
