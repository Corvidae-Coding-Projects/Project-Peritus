//! Concrete bounded filesystem and structured-command developer tools.

use std::{fs, path::PathBuf, process::Command, time::Duration};

use peritus_agent::{DeveloperLoopError, DeveloperToolExecutor, DeveloperToolObservation};
use peritus_model_protocol::CompletedToolCall;
use serde_json::Value;

use super::{
    effect::{atomic_write, atomic_write_if_changed, reject_destructive_command},
    evidence::CommandEvidence,
    grounding::GroundingEvidence,
    inspection,
    ownership::WorkspaceOwnership,
    path::{checked, tool},
    process,
    receipt::{EffectReceiptLedger, ReceiptDecision},
    removal,
    resources::CommandResources,
    wire::{bounded_u64, object, observation, required_string, string},
};
const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_COMMAND_TIMEOUT_SECONDS: u64 = 120;
const MAX_COMMAND_TIMEOUT_SECONDS: u64 = 600;
const TOOLS_WITHOUT_DELIVERY_PROGRESS: u16 = 12;
const MAX_PROGRESS_NUDGES: u8 = 2;
const PROGRESS_FEEDBACK: &str = "The harness observed a long inspection sequence without a workspace mutation or successful declared external effect. Choose the shortest concrete delivery step now. If a standard capability is missing and the active disposable task authorizes installation, use the available package or runtime manager before hand-writing a substitute. Otherwise write or apply the requested result, then verify it. Continue inspecting only when a specific unresolved requirement still needs evidence.";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkspaceToolMode {
    ReadWrite,
    ReadOnly,
}

/// Concrete tool executor scoped to one managed workspace.
pub struct WorkspaceDeveloperTools {
    root: PathBuf,
    grounding: GroundingEvidence,
    ownership: WorkspaceOwnership,
    mode: WorkspaceToolMode,
    command_evidence: CommandEvidence,
    receipts: Option<EffectReceiptLedger>,
    resources: CommandResources,
    tools_without_delivery_progress: u16,
    progress_nudges: u8,
    progress_feedback_pending: bool,
}

impl WorkspaceDeveloperTools {
    /// Creates an executor that rejects every mutating or process tool even if a provider emits an
    /// undeclared call.
    #[must_use]
    pub fn read_only(root: PathBuf) -> Self {
        let ownership = WorkspaceOwnership::capture(&root);
        Self {
            root,
            grounding: GroundingEvidence::default(),
            ownership,
            mode: WorkspaceToolMode::ReadOnly,
            command_evidence: CommandEvidence::default(),
            receipts: None,
            resources: CommandResources::observe(),
            tools_without_delivery_progress: 0,
            progress_nudges: 0,
            progress_feedback_pending: false,
        }
    }

    pub(crate) fn with_ownership(
        root: PathBuf,
        ownership: WorkspaceOwnership,
        receipt_path: PathBuf,
        receipt_scope: String,
    ) -> Self {
        Self {
            root,
            grounding: GroundingEvidence::default(),
            ownership,
            mode: WorkspaceToolMode::ReadWrite,
            command_evidence: CommandEvidence::default(),
            receipts: Some(EffectReceiptLedger::new(receipt_path, receipt_scope)),
            resources: CommandResources::observe(),
            tools_without_delivery_progress: 0,
            progress_nudges: 0,
            progress_feedback_pending: false,
        }
    }

    pub const fn grounding(&self) -> &GroundingEvidence {
        &self.grounding
    }

    pub(crate) const fn ownership(&self) -> &WorkspaceOwnership {
        &self.ownership
    }

    pub(crate) fn verification_evidence(&self) -> String {
        self.command_evidence.render()
    }

    pub(crate) fn successful_commands(&self) -> Vec<super::SuccessfulCommand> {
        self.command_evidence.successful()
    }
}

impl DeveloperToolExecutor for WorkspaceDeveloperTools {
    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        let arguments: Value = serde_json::from_slice(call.arguments().canonical_bytes())
            .map_err(|error| tool(error.to_string()))?;
        let effect = matches!(
            call.name().as_str(),
            "workspace_write" | "workspace_patch" | "workspace_remove" | "run_command"
        ) && self.mode == WorkspaceToolMode::ReadWrite;
        if effect {
            let decision = self
                .receipts
                .as_mut()
                .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
                .begin(call)?;
            match decision {
                ReceiptDecision::Execute => {}
                ReceiptDecision::Replay { value, is_error } => {
                    if value.get("error").is_none() {
                        self.record_success(call.name().as_str(), &arguments, &value);
                    }
                    return observation(&value, is_error);
                }
                ReceiptDecision::Refuse { detail, ambiguous } => {
                    return observation(
                        &object(vec![
                            ("error", Value::String(detail)),
                            ("ambiguous", Value::Bool(ambiguous)),
                        ]),
                        true,
                    );
                }
            }
        }
        let result = match call.name().as_str() {
            "workspace_list" => inspection::list(&self.root, &arguments, self.resources),
            "workspace_search" => inspection::search(&self.root, &arguments),
            "workspace_read" => inspection::read(&self.root, &arguments),
            "workspace_write" | "workspace_patch" | "workspace_remove" | "run_command"
                if self.mode == WorkspaceToolMode::ReadOnly =>
            {
                Err(tool("this role has read-only workspace access"))
            }
            "workspace_write" => self.write(&arguments),
            "workspace_patch" => self.patch(&arguments),
            "workspace_remove" => self.remove(&arguments),
            "run_command" => self.run(&arguments),
            _ => return Err(tool("model requested an undeclared developer tool")),
        };
        let (value, is_error, accepted) = match result {
            Ok(value) => {
                let is_error = value.get("success").and_then(Value::as_bool) == Some(false);
                (value, is_error, true)
            }
            Err(error) => {
                let value = object(vec![("error", Value::String(error.to_string()))]);
                (value, true, false)
            }
        };
        if effect {
            self.receipts
                .as_mut()
                .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
                .complete(&value, is_error)?;
        }
        if accepted {
            self.record_success(call.name().as_str(), &arguments, &value);
        }
        self.observe_delivery_progress(call.name().as_str(), &arguments, &value, accepted);
        observation(&value, is_error)
    }

    fn completion_blocker(&self) -> Option<String> {
        self.grounding.validate().err().map(str::to_owned)
    }

    fn take_progress_feedback(&mut self) -> Option<String> {
        if !std::mem::take(&mut self.progress_feedback_pending) {
            return None;
        }
        Some(PROGRESS_FEEDBACK.to_owned())
    }
}

impl WorkspaceDeveloperTools {
    fn observe_delivery_progress(
        &mut self,
        name: &str,
        arguments: &Value,
        result: &Value,
        accepted: bool,
    ) {
        if self.mode == WorkspaceToolMode::ReadOnly {
            return;
        }
        let workspace_mutation =
            accepted && matches!(name, "workspace_write" | "workspace_patch" | "workspace_remove");
        let external_effect = name == "run_command"
            && result.get("success").and_then(Value::as_bool) == Some(true)
            && string(arguments, "purpose") == Some("external_effect");
        if workspace_mutation || external_effect {
            self.tools_without_delivery_progress = 0;
            self.progress_feedback_pending = false;
            return;
        }
        self.tools_without_delivery_progress =
            self.tools_without_delivery_progress.saturating_add(1);
        if self.tools_without_delivery_progress >= TOOLS_WITHOUT_DELIVERY_PROGRESS
            && self.progress_nudges < MAX_PROGRESS_NUDGES
        {
            self.tools_without_delivery_progress = 0;
            self.progress_nudges = self.progress_nudges.saturating_add(1);
            self.progress_feedback_pending = true;
        }
    }

    fn record_success(&mut self, name: &str, arguments: &Value, result: &Value) {
        match name {
            "workspace_list" => self.grounding.record_list(
                string(arguments, "path").unwrap_or(""),
                result.get("entries").and_then(Value::as_array).map_or(0, Vec::len),
            ),
            "workspace_search" => self.grounding.record_search(),
            "workspace_read" => {
                if let Some(path) = string(arguments, "path") {
                    self.grounding.record_read(path);
                }
            }
            "workspace_write" | "workspace_patch" | "workspace_remove" => {
                if let Some(path) = string(arguments, "path") {
                    self.grounding.record_mutation(path);
                }
            }
            "run_command" => self.command_evidence.record(arguments, result),
            _ => {}
        }
        if name == "workspace_list" {
            for path in result
                .get("entries")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.get("path").and_then(Value::as_str))
            {
                self.grounding.record_listed_path(path);
            }
        }
    }

    fn write(&mut self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let relative = required_string(arguments, "path")?;
        let content = required_string(arguments, "content")?;
        if content.len() > MAX_FILE_BYTES {
            return Err(tool("write exceeds the per-file byte bound"));
        }
        let path = checked(&self.root, relative, true)?;
        let existed_before = path.exists();
        self.grounding.ensure_mutation_allowed(relative, existed_before).map_err(tool)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| tool(error.to_string()))?;
        }
        let changed = atomic_write_if_changed(&path, content.as_bytes())?;
        self.ownership.record_direct_creation(&path, existed_before);
        Ok(object(vec![
            ("path", Value::String(relative.to_owned())),
            ("bytes", Value::from(content.len())),
            ("changed", Value::Bool(changed)),
        ]))
    }

    fn patch(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let relative = required_string(arguments, "path")?;
        let old = required_string(arguments, "old")?;
        let new = required_string(arguments, "new")?;
        let replace_all = arguments.get("replace_all").and_then(Value::as_bool).unwrap_or(false);
        if old.is_empty() {
            return Err(tool("patch old text is empty"));
        }
        let path = checked(&self.root, relative, false)?;
        self.grounding.ensure_mutation_allowed(relative, true).map_err(tool)?;
        let content = fs::read_to_string(&path).map_err(|error| tool(error.to_string()))?;
        let occurrences = content.matches(old).count();
        if occurrences == 0 || (!replace_all && occurrences != 1) {
            return Err(tool(format!("patch expected one match but found {occurrences}")));
        }
        let replaced =
            if replace_all { content.replace(old, new) } else { content.replacen(old, new, 1) };
        if replaced.len() > MAX_FILE_BYTES {
            return Err(tool("patched file exceeds the per-file byte bound"));
        }
        atomic_write(&path, replaced.as_bytes())?;
        Ok(object(vec![
            ("path", Value::String(relative.to_owned())),
            ("replacements", Value::from(if replace_all { occurrences } else { 1 })),
        ]))
    }

    fn remove(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        removal::remove(&self.root, &self.grounding, &self.ownership, arguments)
    }

    fn run(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let program = required_string(arguments, "program")?;
        if program.is_empty() || program.contains(['\0', '\n', '\r']) {
            return Err(tool("command program is invalid"));
        }
        let args = arguments
            .get("args")
            .and_then(Value::as_array)
            .ok_or_else(|| tool("command args must be an array"))?
            .iter()
            .map(|value| {
                value.as_str().map(str::to_owned).ok_or_else(|| tool("command arg is not text"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        reject_destructive_command(program, &args)?;
        let current_dir = match string(arguments, "cwd") {
            Some(value) if !value.is_empty() => checked(&self.root, value, false)?,
            _ => self.root.clone(),
        };
        let mut command = Command::new(program);
        command.args(&args).current_dir(current_dir);
        self.resources.prepare(&mut command, program, &args)?;
        let timeout_seconds = bounded_u64(
            arguments,
            "timeout_seconds",
            DEFAULT_COMMAND_TIMEOUT_SECONDS,
            1,
            MAX_COMMAND_TIMEOUT_SECONDS,
        );
        let output = process::run(command, Duration::from_secs(timeout_seconds))?;
        let recovery_hint = output.timed_out.then_some(
            "Do not retry an equivalent command with a longer timeout or another bulk-transfer \
             wrapper without new size or progress evidence. Preserve the task deadline and choose \
             a materially bounded or resumable strategy.",
        );
        let result = object(vec![
            ("success", Value::Bool(output.status.success() && !output.timed_out)),
            ("exit_code", output.status.code().map_or(Value::Null, Value::from)),
            ("stdout", Value::String(output.stdout)),
            ("stderr", Value::String(output.stderr)),
            ("timed_out", Value::Bool(output.timed_out)),
            ("timeout_seconds", Value::from(timeout_seconds)),
            (
                "recovery_hint",
                recovery_hint.map_or(Value::Null, |value| Value::String(value.to_owned())),
            ),
        ]);
        Ok(result)
    }
}

#[cfg(test)]
mod receipt_tests;
#[cfg(test)]
mod tests;
