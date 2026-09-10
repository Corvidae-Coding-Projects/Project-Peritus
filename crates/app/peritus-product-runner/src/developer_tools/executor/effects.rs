//! Effect receipts, delivery progress, and bounded filesystem mutations.

use super::{
    CompletedToolCall, DeveloperLoopError, DeveloperToolObservation, MAX_FILE_BYTES,
    MAX_PROGRESS_NUDGES, ReceiptDecision, TOOLS_WITHOUT_DELIVERY_PROGRESS, Value,
    WorkspaceDeveloperTools, WorkspaceToolMode, atomic_write, checked, fs, object, observation,
    removal, required_string, string, tool,
};

impl WorkspaceDeveloperTools {
    pub(super) fn replay_effect(
        &mut self,
        call: &CompletedToolCall,
        arguments: &Value,
    ) -> Result<Option<DeveloperToolObservation>, DeveloperLoopError> {
        let Some(decision) = self
            .receipts
            .as_mut()
            .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
            .replay(call)?
        else {
            return Ok(None);
        };
        match decision {
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
            ReceiptDecision::Execute => Err(tool("receipt replay returned an execute decision")),
        }
    }

    pub(super) fn dispatch_tool(
        &mut self,
        call: &CompletedToolCall,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        use super::inspection;
        match call.name().as_str() {
            "workspace_list" => {
                inspection::list(&self.root, arguments, self.resources, &self.access_policy)
            }
            "workspace_search" => inspection::search(&self.root, arguments, &self.access_policy),
            "workspace_read" => inspection::read(&self.root, arguments),
            "workspace_scope" => self.declare_in_place(arguments),
            "workspace_write" => self.write(arguments),
            "workspace_patch" => self.patch(arguments),
            "workspace_remove" => self.remove(arguments),
            "run_command" => self.run_command(arguments, call.id().expose_for_wire()),
            "command_start" => self.start_command(arguments, call.id().expose_for_wire()),
            "command_poll" => self.poll_command(arguments),
            "command_stdin" => self.write_command_stdin(arguments),
            "command_resize" => self.resize_command(arguments),
            "command_signal" => self.signal_command(arguments),
            "command_cancel" => self.cancel_command(arguments),
            "command_recover" => self.recover_command(arguments),
            _ => Err(tool("model requested an undeclared developer tool")),
        }
    }

    pub(super) fn begin_effect(
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
    pub(super) fn observe_delivery_progress(
        &mut self,
        name: &str,
        arguments: &Value,
        result: &Value,
        accepted: bool,
    ) {
        if self.mode == WorkspaceToolMode::ReadOnly {
            return;
        }
        let workspace_mutation = accepted
            && matches!(name, "workspace_write" | "workspace_patch" | "workspace_remove")
            && result.get("changed").and_then(Value::as_bool) != Some(false);
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
            self.progress_nudges = 0;
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

    pub(super) fn record_success(&mut self, name: &str, arguments: &Value, result: &Value) {
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

    pub(super) fn write(&mut self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let relative = required_string(arguments, "path")?;
        let content = required_string(arguments, "content")?;
        if content.len() > MAX_FILE_BYTES {
            return Err(tool("write exceeds the per-file byte bound"));
        }
        let path = checked(&self.root, relative, true)?;
        let existed_before = path.exists();
        self.grounding.ensure_mutation_allowed(relative, existed_before).map_err(tool)?;
        if path.is_file()
            && fs::read(&path).map_err(|error| tool(error.to_string()))? == content.as_bytes()
        {
            return Ok(object(vec![
                ("path", Value::String(relative.to_owned())),
                ("bytes", Value::from(content.len())),
                ("changed", Value::Bool(false)),
            ]));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| tool(error.to_string()))?;
        }
        atomic_write(&path, content.as_bytes())?;
        self.ownership.record_direct_creation(&path, existed_before);
        Ok(object(vec![
            ("path", Value::String(relative.to_owned())),
            ("bytes", Value::from(content.len())),
            ("changed", Value::Bool(true)),
        ]))
    }

    pub(super) fn patch(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
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

    pub(super) fn remove(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        removal::remove(&self.root, &self.grounding, &self.ownership, arguments)
    }
}
