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
    wire::{bounded_u64, object, observation, required_string, string},
};
const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_COMMAND_TIMEOUT_SECONDS: u64 = 120;
const MAX_COMMAND_TIMEOUT_SECONDS: u64 = 600;

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
            "workspace_list" => inspection::list(&self.root, &arguments),
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
        observation(&value, is_error)
    }

    fn completion_blocker(&self) -> Option<String> {
        self.grounding.validate().err().map(str::to_owned)
    }
}

impl WorkspaceDeveloperTools {
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
        if program == "cargo" {
            command.env("CARGO_BUILD_JOBS", "2");
        }
        let timeout_seconds = bounded_u64(
            arguments,
            "timeout_seconds",
            DEFAULT_COMMAND_TIMEOUT_SECONDS,
            1,
            MAX_COMMAND_TIMEOUT_SECONDS,
        );
        let output = process::run(command, Duration::from_secs(timeout_seconds))?;
        let result = object(vec![
            ("success", Value::Bool(output.status.success() && !output.timed_out)),
            ("exit_code", output.status.code().map_or(Value::Null, Value::from)),
            ("stdout", Value::String(output.stdout)),
            ("stderr", Value::String(output.stderr)),
            ("timed_out", Value::Bool(output.timed_out)),
            ("timeout_seconds", Value::from(timeout_seconds)),
        ]);
        Ok(result)
    }
}

#[cfg(test)]
mod receipt_tests;
#[cfg(test)]
mod tests;
