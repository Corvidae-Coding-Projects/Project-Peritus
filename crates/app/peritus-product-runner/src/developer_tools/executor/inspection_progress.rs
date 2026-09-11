//! Bounded, content-based detection of an unchanged inspection cycle.

use std::collections::VecDeque;

use peritus_types::Sha256Digest;
use serde_json::Value;

const RECENT_OBSERVATIONS: usize = 16;
const WARN_REPEATS: u8 = 3;
const STOP_REPEATS: u8 = 6;

#[derive(Default)]
pub(super) struct InspectionProgress {
    recent: VecDeque<Sha256Digest>,
    repeats: u8,
    warning_pending: bool,
}

impl InspectionProgress {
    pub(super) fn observe(&mut self, name: &str, arguments: &Value, result: &Value) {
        if !matches!(name, "workspace_list" | "workspace_read" | "workspace_search") {
            // Commands (including polling and failed verification), edits, and other tool
            // strategies end this inspection sequence. Never infer process progress here.
            *self = Self::default();
            return;
        }
        let digest = peritus_codec::sha256(format!("{name}\n{arguments}\n{result}").as_bytes());
        if self.recent.contains(&digest) {
            self.repeats = self.repeats.saturating_add(1);
            if self.repeats == WARN_REPEATS {
                self.warning_pending = true;
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
            "The host detected repeated identical workspace inspection arguments AND results with no new evidence. Initial grounding is not reset between provider steps. Use the observations already obtained and take a different task-relevant step, or identify the specific missing evidence. Repeating this unchanged inspection cycle will stop the invocation with an inspection-no-progress error; it will not trigger fresh-invocation recovery.".to_owned()
        })
    }

    pub(super) fn blocker(&self) -> Option<String> {
        (self.repeats >= STOP_REPEATS).then(|| {
            format!(
                "inspection-no-progress: {} consecutive workspace inspections repeated previously observed arguments and results without new evidence. Stopped after warning; completed work and tool observations are preserved. Review the last tool results before starting another invocation.",
                self.repeats,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: impl Into<Value>) -> Value {
        Value::from_iter([("path", value.into())])
    }

    #[test]
    fn identical_and_alternating_inspections_warn_then_stop() {
        for width in [1, 2] {
            let mut progress = InspectionProgress::default();
            for index in 0..width {
                progress.observe("workspace_read", &path(index), &Value::from("same"));
            }
            for index in 1..=STOP_REPEATS {
                progress.observe("workspace_read", &path(index % width), &Value::from("same"));
                assert_eq!(progress.feedback().is_some(), index == WARN_REPEATS);
                assert_eq!(progress.blocker().is_some(), index == STOP_REPEATS);
            }
            assert!(progress.feedback().is_none());
        }
    }

    #[test]
    fn new_evidence_and_noninspection_work_reset_the_cycle() {
        let mut progress = InspectionProgress::default();
        for index in 0..32 {
            progress.observe("workspace_read", &path("state"), &Value::from(index));
            assert!(progress.blocker().is_none());
        }
        assert_eq!(progress.recent.len(), RECENT_OBSERVATIONS);
        for name in ["run_command", "command_poll", "workspace_write"] {
            for _ in 0..=STOP_REPEATS {
                progress.observe("workspace_list", &path("."), &Value::Array(Vec::new()));
            }
            assert!(progress.blocker().is_some());
            progress.observe(
                name,
                &Value::Object(serde_json::Map::new()),
                &Value::from_iter([("success", Value::from(false))]),
            );
            assert!(progress.blocker().is_none());
            assert!(progress.feedback().is_none());
        }
    }
}
