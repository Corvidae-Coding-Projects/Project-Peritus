//! Bounded developer-command observations carried into independent review.

use std::collections::VecDeque;

use serde_json::Value;

/// Exact immutable source sections from one independent reviewer request.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReviewerEvidenceSources {
    transcript: String,
    diff: String,
    gates: String,
    developer_commands: String,
    prior: String,
    correction: String,
}

impl ReviewerEvidenceSources {
    pub const fn new(
        transcript: String,
        diff: String,
        gates: String,
        developer_commands: String,
        prior: String,
        correction: String,
    ) -> Self {
        Self { transcript, diff, gates, developer_commands, prior, correction }
    }

    pub fn section(&self, name: &str) -> Option<&str> {
        match name {
            "transcript" => Some(&self.transcript),
            "diff" => Some(&self.diff),
            "gates" => Some(&self.gates),
            "developer_commands" => Some(&self.developer_commands),
            "prior" => Some(&self.prior),
            "correction" => Some(&self.correction),
            _ => None,
        }
    }
}

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
    successful: Vec<SuccessfulCommand>,
}

impl CommandEvidence {
    pub(super) fn record(&mut self, arguments: &Value, result: &Value) {
        self.record_named("run_command", arguments, result);
    }

    pub(super) fn record_named(&mut self, tool: &str, arguments: &Value, result: &Value) {
        let request = serde_json::to_string(arguments).unwrap_or_else(|_| "<invalid JSON>".into());
        let rendered_result =
            serde_json::to_string(result).unwrap_or_else(|_| "<invalid JSON>".into());
        let record = format!("tool: {tool}\nrequest: {request}\nresult: {rendered_result}");
        self.records.push_back(record);
        if result.get("success").and_then(Value::as_bool) == Some(true)
            && let Some(purpose) = CommandPurpose::from_arguments(arguments)
        {
            self.successful
                .push(SuccessfulCommand { command: format!("{tool} {request}"), purpose });
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
        self.successful.clone()
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
}

/// Merges successful commands while preserving their execution order.
pub fn merge_successful(retained: &mut Vec<SuccessfulCommand>, incoming: &[SuccessfulCommand]) {
    retained.extend_from_slice(incoming);
}
