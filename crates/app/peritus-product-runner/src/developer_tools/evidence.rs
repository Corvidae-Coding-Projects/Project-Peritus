//! Bounded developer-command observations carried into independent review.

use std::collections::VecDeque;

use serde_json::Value;

const MAX_RECORDS: usize = 32;
const MAX_RECORD_BYTES: usize = 16 * 1024;
const MAX_RENDERED_BYTES: usize = 128 * 1024;

/// Declared role of one successful structured command in delivery acceptance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandPurpose {
    /// Performs a caller-authorized effect outside the managed workspace.
    ExternalEffect,
    /// Freshly inspects the resulting state or exercises the requested behavior end to end.
    Verification,
}

/// One successful bounded command retained in exact completion evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuccessfulCommand {
    /// Canonical structured command request.
    pub command: String,
    /// Acceptance role declared by the developer when it requested the command.
    pub purpose: CommandPurpose,
}

#[derive(Default)]
pub(super) struct CommandEvidence {
    records: VecDeque<String>,
    bytes: usize,
    successful: VecDeque<SuccessfulCommand>,
}

impl CommandEvidence {
    pub(super) fn record(&mut self, arguments: &Value, result: &Value) {
        let request = serde_json::to_string(arguments).unwrap_or_else(|_| "<invalid JSON>".into());
        let rendered_result =
            serde_json::to_string(result).unwrap_or_else(|_| "<invalid JSON>".into());
        let record = format!(
            "request: {}\nresult: {}",
            preview(&request, MAX_RECORD_BYTES / 4),
            preview(&rendered_result, MAX_RECORD_BYTES * 3 / 4),
        );
        self.bytes = self.bytes.saturating_add(record.len());
        self.records.push_back(record);
        while self.records.len() > MAX_RECORDS || self.bytes > MAX_RENDERED_BYTES {
            let Some(removed) = self.records.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(removed.len());
        }
        if result.get("success").and_then(Value::as_bool) == Some(true)
            && let Some(purpose) = CommandPurpose::from_arguments(arguments)
        {
            self.successful.push_back(SuccessfulCommand {
                command: format!("run_command {request}"),
                purpose,
            });
            while self.successful.len() > MAX_RECORDS {
                self.successful.pop_front();
            }
        }
    }

    pub(super) fn render(&self) -> String {
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| format!("[Developer command {}]\n{record}", index + 1))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub(super) fn successful(&self) -> Vec<SuccessfulCommand> {
        self.successful.iter().cloned().collect()
    }
}

impl CommandPurpose {
    fn from_arguments(arguments: &Value) -> Option<Self> {
        match arguments.get("purpose").and_then(Value::as_str) {
            Some("external_effect") => Some(Self::ExternalEffect),
            Some("verification") => Some(Self::Verification),
            _ => None,
        }
    }
}

pub(super) fn merge_rendered(retained: &mut String, incoming: &str) {
    if incoming.is_empty() {
        return;
    }
    if !retained.is_empty() {
        retained.push_str("\n\n");
    }
    retained.push_str(incoming);
    if retained.len() > MAX_RENDERED_BYTES {
        let start = retained.len() - MAX_RENDERED_BYTES;
        let boundary = retained.ceil_char_boundary(start);
        *retained =
            format!("[earlier developer command evidence omitted]\n{}", &retained[boundary..]);
    }
}

/// Merges recent successful commands while preserving their execution order and memory bound.
pub fn merge_successful(retained: &mut Vec<SuccessfulCommand>, incoming: &[SuccessfulCommand]) {
    retained.extend_from_slice(incoming);
    if retained.len() > MAX_RECORDS {
        retained.drain(..retained.len() - MAX_RECORDS);
    }
}

fn preview(value: &str, maximum: usize) -> String {
    if value.len() <= maximum {
        return value.to_owned();
    }
    let boundary = value.floor_char_boundary(maximum);
    format!("{}...[truncated]", &value[..boundary])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_retains_recent_bounded_command_observations() {
        let mut evidence = CommandEvidence::default();
        let result: Value =
            serde_json::from_str(r#"{"success":true,"stdout":"verified"}"#).expect("result");
        for index in 0..40 {
            let request: Value = serde_json::from_str(&format!(
                r#"{{"program":"python","args":["check-{index}.py"]}}"#
            ))
            .expect("request");
            evidence.record(&request, &result);
        }

        let rendered = evidence.render();
        assert!(!rendered.contains("check-0.py"));
        assert!(rendered.contains("check-39.py"));
        assert!(rendered.len() <= MAX_RENDERED_BYTES);
    }

    #[test]
    fn only_explicit_successful_purposes_become_acceptance_evidence() {
        let mut evidence = CommandEvidence::default();
        let success: Value = serde_json::from_str(r#"{"success":true}"#).expect("success");
        let failure: Value = serde_json::from_str(r#"{"success":false}"#).expect("failure");
        let effect: Value = serde_json::from_str(
            r#"{"args":["apply"],"program":"admin","purpose":"external_effect"}"#,
        )
        .expect("effect");
        let verification: Value = serde_json::from_str(
            r#"{"args":["status"],"program":"admin","purpose":"verification"}"#,
        )
        .expect("verification");
        let unlabeled: Value =
            serde_json::from_str(r#"{"args":[],"program":"true"}"#).expect("unlabeled");

        evidence.record(&effect, &success);
        evidence.record(&verification, &failure);
        evidence.record(&unlabeled, &success);

        let retained = evidence.successful();
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].purpose, CommandPurpose::ExternalEffect);
        assert!(retained[0].command.contains("admin"));
    }
}
