//! Evidence and workspace ownership for commands completed through an active handle.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use crate::developer_tools::{
    evidence::CommandEvidence, ownership::WorkspaceOwnership, path::tool,
};

#[derive(Default)]
pub(super) struct ActiveCommandLedger {
    commands: BTreeMap<String, ActiveCommand>,
}

struct ActiveCommand {
    request: Value,
    unowned_before: BTreeSet<PathBuf>,
    recorded: bool,
}

impl ActiveCommandLedger {
    pub(super) fn started(
        &mut self,
        request: &Value,
        result: &Value,
        unowned_before: BTreeSet<PathBuf>,
    ) -> Result<(), DeveloperLoopError> {
        let handle = result
            .get("handle")
            .and_then(Value::as_str)
            .ok_or_else(|| tool("active command start returned no handle"))?;
        if self
            .commands
            .insert(
                handle.to_owned(),
                ActiveCommand { request: request.clone(), unowned_before, recorded: false },
            )
            .is_some()
        {
            return Err(tool("active command start reused a live handle"));
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
            ownership.record_command_creations(root, &command.unowned_before);
            evidence.record_named("command_start", &command.request, result);
            command.recorded = true;
        }
        Ok(())
    }
}
