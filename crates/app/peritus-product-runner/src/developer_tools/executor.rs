//! Concrete bounded filesystem and structured-command developer tools.

use std::{fs, path::PathBuf};

use peritus_agent::{DeveloperLoopError, DeveloperToolExecutor, DeveloperToolObservation};
use peritus_model_protocol::CompletedToolCall;
use serde_json::Value;

use super::{
    access_policy::WorkspaceAccessPolicy,
    command_budget::CommandBudget,
    effect::{atomic_write, atomic_write_if_changed},
    evidence::CommandEvidence,
    grounding::GroundingEvidence,
    inspection,
    ownership::WorkspaceOwnership,
    path::{checked, tool},
    receipt::{EffectReceiptLedger, ReceiptDecision},
    removal,
    resources::CommandResources,
    wire::{object, observation, required_string, string},
};
const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
const TOOLS_WITHOUT_DELIVERY_PROGRESS: u16 = 12;
const MAX_PROGRESS_NUDGES: u8 = 2;
const PROGRESS_FEEDBACK: &str = "The harness observed a long inspection sequence without a workspace mutation or successful declared external effect. Choose the shortest concrete delivery step now. If a standard capability is missing and the active disposable task authorizes installation, use the available package or runtime manager before hand-writing a substitute. Otherwise write or apply the requested result, then verify it. Continue inspecting only when a specific unresolved requirement still needs evidence.";

mod active;
mod checkpoint_observer;
mod command;
mod construction;
mod in_place;

use active::ActiveCommandLedger;
pub use checkpoint_observer::ToolCheckpointBoundary;
use checkpoint_observer::ToolCheckpointObserver;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkspaceToolMode {
    ReadWrite,
    ReadOnly,
}

/// Concrete tool executor scoped to one managed workspace.
pub struct WorkspaceDeveloperTools {
    pub(super) root: PathBuf,
    pub(super) access_policy: WorkspaceAccessPolicy,
    grounding: GroundingEvidence,
    ownership: WorkspaceOwnership,
    mode: WorkspaceToolMode,
    in_place_scope: Option<crate::workspace_delivery::scope::ScopedBaseline>,
    command_evidence: CommandEvidence,
    command_budget: Option<CommandBudget>,
    receipts: Option<EffectReceiptLedger>,
    resources: CommandResources,
    command_runtime: Option<crate::CommandRuntime>,
    active_commands: ActiveCommandLedger,
    tools_without_delivery_progress: u16,
    progress_nudges: u8,
    progress_feedback_pending: bool,
    checkpoint_observer: Option<ToolCheckpointObserver>,
}

impl WorkspaceDeveloperTools {
    pub(crate) fn with_in_place_scope(
        mut self,
        scope: Option<crate::workspace_delivery::scope::ScopedBaseline>,
    ) -> Self {
        self.in_place_scope = scope;
        self
    }

    pub(crate) fn with_checkpoint_observer(mut self, observer: ToolCheckpointObserver) -> Self {
        self.checkpoint_observer = Some(observer);
        self
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

#[cfg(test)]
fn test_command_runtime(root: &std::path::Path) -> crate::CommandRuntime {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&NEXT.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    crate::CommandRuntime::open_for_test(
        root,
        peritus_types::RunId::new(bytes).expect("test command run ID"),
    )
}

impl DeveloperToolExecutor for WorkspaceDeveloperTools {
    fn required_tool_name(&self) -> Option<&str> {
        self.grounding.required_tool_name()
    }

    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        let arguments: Value = serde_json::from_slice(call.arguments().canonical_bytes())
            .map_err(|error| tool(error.to_string()))?;
        if let Err(detail) = self.access_policy.authorize(call.name().as_str(), &arguments) {
            return observation(&object(vec![("error", Value::String(detail))]), true);
        }
        if let Err(error) = self.prepare_in_place(call.name().as_str(), &arguments) {
            return observation(&object(vec![("error", Value::String(error.to_string()))]), true);
        }
        let effect = matches!(
            call.name().as_str(),
            "workspace_write"
                | "workspace_patch"
                | "workspace_remove"
                | "run_command"
                | "command_start"
                | "command_stdin"
                | "command_resize"
                | "command_signal"
                | "command_cancel"
        ) && self.mode == WorkspaceToolMode::ReadWrite;
        if effect && let Some(observation) = self.begin_effect(call, &arguments)? {
            return Ok(observation);
        }
        let result = match call.name().as_str() {
            "workspace_list" => {
                inspection::list(&self.root, &arguments, self.resources, &self.access_policy)
            }
            "workspace_search" => inspection::search(&self.root, &arguments, &self.access_policy),
            "workspace_read" => inspection::read(&self.root, &arguments),
            "workspace_scope" => self.declare_in_place(&arguments),
            "workspace_write" | "workspace_patch" | "workspace_remove" | "run_command"
            | "command_start" | "command_stdin" | "command_resize" | "command_signal"
            | "command_cancel" | "command_poll" | "command_recover"
                if self.mode == WorkspaceToolMode::ReadOnly =>
            {
                Err(tool("this role has read-only workspace access"))
            }
            "workspace_write" => self.write(&arguments),
            "workspace_patch" => self.patch(&arguments),
            "workspace_remove" => self.remove(&arguments),
            "run_command" => self.run_command(&arguments, call.id().expose_for_wire()),
            "command_start" => self.start_command(&arguments, call.id().expose_for_wire()),
            "command_poll" => self.poll_command(&arguments),
            "command_stdin" => self.write_command_stdin(&arguments),
            "command_resize" => self.resize_command(&arguments),
            "command_signal" => self.signal_command(&arguments),
            "command_cancel" => self.cancel_command(&arguments),
            "command_recover" => self.recover_command(&arguments),
            _ => return Err(tool("model requested an undeclared developer tool")),
        };
        let (mut value, is_error, accepted) = match result {
            Ok(value) => {
                let is_error = value.get("success").and_then(Value::as_bool) == Some(false);
                (value, is_error, true)
            }
            Err(error) => {
                let value = object(vec![("error", Value::String(error.to_string()))]);
                (value, true, false)
            }
        };
        if accepted {
            self.active_commands.observe(
                &self.root,
                &mut value,
                &mut self.ownership,
                &mut self.command_evidence,
            )?;
        }
        if effect {
            self.receipts
                .as_mut()
                .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
                .complete(&value, is_error)?;
        }
        if accepted {
            self.record_checkpoint(call.name().as_str(), &arguments, &value)?;
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
    fn begin_effect(
        &mut self,
        call: &CompletedToolCall,
        arguments: &Value,
    ) -> Result<Option<DeveloperToolObservation>, DeveloperLoopError> {
        let decision = self
            .receipts
            .as_mut()
            .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
            .begin(call)?;
        match decision {
            ReceiptDecision::Execute => Ok(None),
            ReceiptDecision::Replay { value, is_error } => {
                if value.get("error").is_none() {
                    self.record_success(call.name().as_str(), arguments, &value);
                }
                observation(&value, is_error).map(Some)
            }
            ReceiptDecision::Refuse { detail, ambiguous } => observation(
                &object(vec![
                    ("error", Value::String(detail)),
                    ("ambiguous", Value::Bool(ambiguous)),
                ]),
                true,
            )
            .map(Some),
        }
    }
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
        let external_effect = matches!(
            name,
            "run_command"
                | "command_poll"
                | "command_stdin"
                | "command_resize"
                | "command_signal"
                | "command_cancel"
                | "command_recover"
        ) && result.get("success").and_then(Value::as_bool) == Some(true)
            && (string(arguments, "purpose") == Some("external_effect")
                || result.get("purpose").and_then(Value::as_str) == Some("external_effect"));
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
                    self.ownership.observe_file(self.root.join(path));
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
}

#[cfg(test)]
mod receipt_tests;
#[cfg(test)]
mod tests;
