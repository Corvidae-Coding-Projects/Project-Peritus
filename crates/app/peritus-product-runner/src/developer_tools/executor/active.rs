//! Evidence and workspace ownership for commands completed through an active handle.

use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    path::{Path, PathBuf},
};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use crate::developer_tools::{
    command_runtime::CommandExecutionMode, evidence::CommandEvidence,
    ownership::WorkspaceOwnership, path::tool,
};

#[derive(Default)]
pub(super) struct ActiveCommandLedger {
    commands: BTreeMap<String, ActiveCommand>,
}

struct ActiveCommand {
    tool: &'static str,
    request: Value,
    unowned_before: BTreeSet<PathBuf>,
    recorded: bool,
    checkpointed: bool,
    mode: CommandExecutionMode,
}

impl ActiveCommandLedger {
    pub(super) fn started(
        &mut self,
        request: &Value,
        result: &Value,
        unowned_before: BTreeSet<PathBuf>,
        mode: CommandExecutionMode,
    ) -> Result<(), DeveloperLoopError> {
        self.started_named("command_start", request, result, unowned_before, mode)
    }

    pub(super) fn started_named(
        &mut self,
        tool_name: &'static str,
        request: &Value,
        result: &Value,
        unowned_before: BTreeSet<PathBuf>,
        mode: CommandExecutionMode,
    ) -> Result<(), DeveloperLoopError> {
        let handle = result
            .get("handle")
            .and_then(Value::as_str)
            .ok_or_else(|| tool("active command start returned no handle"))?;
        match self.commands.entry(handle.to_owned()) {
            Entry::Vacant(slot) => {
                slot.insert(ActiveCommand {
                    tool: tool_name,
                    request: request.clone(),
                    unowned_before,
                    recorded: false,
                    checkpointed: false,
                    mode,
                });
            }
            Entry::Occupied(_) => return Err(tool("active command start reused a live handle")),
        }
        Ok(())
    }

    pub(super) fn observe(
        &mut self,
        root: &Path,
        result: &mut Value,
        ownership: &mut WorkspaceOwnership,
        evidence: &mut CommandEvidence,
    ) -> Result<(), DeveloperLoopError> {
        let state = result.get("state").and_then(Value::as_str);
        if !matches!(state, Some("completed" | "indeterminate")) {
            return Ok(());
        }
        let handle = result
            .get("handle")
            .and_then(Value::as_str)
            .ok_or_else(|| tool("terminal active command result has no handle"))?;
        let Some(command) = self.commands.get_mut(handle) else {
            return Ok(());
        };
        if let Some(purpose) = command.request.get("purpose").cloned() {
            result
                .as_object_mut()
                .ok_or_else(|| tool("terminal active command result is not an object"))?
                .insert("purpose".to_owned(), purpose);
        }
        if !command.recorded {
            if command.mode.is_mutation() {
                ownership.record_command_creations(root, &command.unowned_before)?;
            }
            evidence.record_named(command.tool, &command.request, result);
            command.recorded = true;
        }
        Ok(())
    }

    pub(super) fn pending_checkpoint(&self, result: &Value) -> Option<(String, Value)> {
        if !matches!(result.get("state").and_then(Value::as_str), Some("completed" | "indeterminate")) {
            return None;
        }
        let handle = result.get("handle")?.as_str()?;
        let command = self.commands.get(handle)?;
        if command.checkpointed {
            return None;
        }
        Some((handle.to_owned(), command.request.clone()))
    }

    pub(super) fn checkpoint_completed(&mut self, handle: &str) -> Result<(), DeveloperLoopError> {
        let command = self.commands.get_mut(handle)
            .ok_or_else(|| tool("terminal command checkpoint lost its retained owner"))?;
        command.checkpointed = true;
        Ok(())
    }
}
