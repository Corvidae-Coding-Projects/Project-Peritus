//! Effect receipts, delivery progress, and bounded filesystem mutations.

use super::{
    CompletedToolCall, DeveloperLoopError, DeveloperToolObservation, MAX_PROGRESS_NUDGES,
    ReceiptDecision, TOOLS_WITHOUT_DELIVERY_PROGRESS, Value, WorkspaceDeveloperTools,
    WorkspaceToolMode, atomic_write, checked, fs, object, observation, removal, required_string,
    string, tool,
};

impl WorkspaceDeveloperTools {
    pub(super) fn replay_effect(
        &mut self,
        call: &CompletedToolCall,
        arguments: &Value,
    ) -> Result<Option<DeveloperToolObservation>, DeveloperLoopError> {
        let owners = self
            .receipts
            .as_mut()
            .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
            .uncertain_command_owners()?;
        if !owners.is_empty() {
            let runtime = self
                .command_runtime
                .as_ref()
                .ok_or_else(|| tool("writable tools have no command runtime"))?;
            for owner in owners {
                let recovered = runtime.recover_receipt_owner(owner)?;
                self.receipts
                    .as_mut()
                    .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
                    .reconcile_native_command_owner(
                        owner,
                        recovered.disposition,
                        &recovered.value,
                    )?;
            }
        }
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
                if value.get("error").is_none()
                    && value.get("success").and_then(Value::as_bool) != Some(false)
                {
                    self.record_success(call.name().as_str(), arguments, &value);
                }
                observation(&value, is_error).map(Some)
            }
            ReceiptDecision::RecoverCheckpoint { value, is_error } => {
                self.recover_checkpoint(call.name().as_str(), arguments, &value)?;
                self.receipts
                    .as_mut()
                    .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
                    .finalize()?;
                if value.get("error").is_none()
                    && value.get("success").and_then(Value::as_bool) != Some(false)
                {
                    self.record_success(call.name().as_str(), arguments, &value);
                }
                observation(&value, is_error).map(Some)
            }
            ReceiptDecision::RecoverCommandOwner { owner, scope, ordinal } => {
                self.recover_command_receipt(call, arguments, owner, &scope, ordinal).map(Some)
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
        use super::super::inspection_search;
        use super::inspection;
        use crate::developer_tools::reference;
        match call.name().as_str() {
            "workspace_list" if routes_to_reference(&self.root, arguments) => {
                reference::list(&self.references, arguments, &self.inspection_cancellation)
            }
            "workspace_list" => inspection::list(
                &self.root,
                arguments,
                self.resources,
                &self.access_policy,
                &self.inspection_cancellation,
            ),
            "workspace_search" => inspection_search::search(
                &self.root,
                arguments,
                &self.access_policy,
                &self.inspection_cancellation,
            ),
            "workspace_read" if routes_to_reference(&self.root, arguments) => {
                reference::read(&self.references, arguments, &self.inspection_cancellation)
            }
            "workspace_read" => {
                inspection::read(&self.root, arguments, &self.inspection_cancellation)
            }
            "attachment_read" => {
                super::super::attachment_read::read(self.protection_view.as_deref(), arguments)
            }
            "developer_evidence_read" => self.read_reviewer_evidence(arguments),
            "workspace_scope" => self.declare_in_place(arguments),
            "workspace_write" => self.write(arguments),
            "workspace_patch" => self.patch(arguments),
            "workspace_remove" => self.remove(arguments),
            "run_command" => self.run_command(arguments, call.id().expose_for_wire()),
            "command_start" => self.start_command(arguments, call.id().expose_for_wire()),
            "command_poll" => {
                self.attach_command_owner_from_handle(arguments)?;
                self.poll_command(arguments)
            }
            "command_stdin" => {
                self.bind_command_receipt_owner(arguments, call.id().expose_for_wire())?;
                self.write_command_stdin(arguments)
            }
            "command_resize" => {
                self.bind_command_receipt_owner(arguments, call.id().expose_for_wire())?;
                self.resize_command(arguments)
            }
            "command_signal" => {
                self.bind_command_receipt_owner(arguments, call.id().expose_for_wire())?;
                self.signal_command(arguments)
            }
            "command_cancel" => {
                self.bind_command_receipt_owner(arguments, call.id().expose_for_wire())?;
                self.cancel_command(arguments)
            }
            "command_recover" => {
                self.attach_command_owner_from_handle(arguments)?;
                self.recover_command(arguments)
            }
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
            ReceiptDecision::RecoverCheckpoint { .. } => {
                Err(tool("checkpoint recovery must occur before effect preflight"))
            }
            ReceiptDecision::RecoverCommandOwner { owner, scope, ordinal } => {
                self.recover_command_receipt(call, arguments, owner, &scope, ordinal).map(Some)
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

    fn bind_command_receipt_owner(
        &mut self,
        arguments: &Value,
        call_id: &str,
    ) -> Result<(), DeveloperLoopError> {
        let handle = required_string(arguments, "handle")?;
        let owner = {
            let receipts = self
                .receipts
                .as_mut()
                .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?;
            let owner = receipts
                .command_owner_for_handle(handle)?
                .ok_or_else(|| tool("command handle has no durable native owner receipt"))?;
            receipts.bind_native_command_owner(call_id, owner)?;
            owner
        };
        self.command_runtime
            .as_ref()
            .ok_or_else(|| tool("writable tools have no command runtime"))?
            .attach_native_owner(owner)
    }

    fn attach_command_owner_from_handle(
        &mut self,
        arguments: &Value,
    ) -> Result<(), DeveloperLoopError> {
        let handle = required_string(arguments, "handle")?;
        let owner = self
            .receipts
            .as_mut()
            .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
            .command_owner_for_handle(handle)?;
        if let Some(owner) = owner {
            self.command_runtime
                .as_ref()
                .ok_or_else(|| tool("writable tools have no command runtime"))?
                .attach_native_owner(owner)?;
        }
        Ok(())
    }

    fn recover_command_receipt(
        &mut self,
        call: &CompletedToolCall,
        arguments: &Value,
        owner: super::super::receipt::NativeCommandOwner,
        scope: &str,
        ordinal: u32,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        let runtime = self
            .command_runtime
            .as_ref()
            .ok_or_else(|| tool("writable tools have no command runtime"))?;
        let mut recovered = runtime.recover_receipt_owner(owner)?;
        if call.name().as_str() == "run_command" {
            while recovered.disposition == peritus_process::RecoveryDisposition::LiveOwned {
                std::thread::sleep(std::time::Duration::from_millis(20));
                recovered = runtime.recover_receipt_owner(owner)?;
            }
        }
        let tool_name = call.name().as_str();
        let is_start = tool_name == "command_start";
        let terminal = recovered.disposition == peritus_process::RecoveryDisposition::Terminal;
        let live_start =
            is_start && recovered.disposition == peritus_process::RecoveryDisposition::LiveOwned;
        if terminal && matches!(tool_name, "run_command" | "command_start") || live_start {
            let is_error = recovered.value.get("success").and_then(Value::as_bool) == Some(false);
            self.receipts
                .as_mut()
                .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
                .complete_recovered_command(
                    scope,
                    ordinal,
                    owner,
                    &recovered.value,
                    is_error,
                    terminal,
                )?;
            if !is_error {
                self.record_success(tool_name, arguments, &recovered.value);
            }
            return observation(&recovered.value, is_error);
        }
        let owner_inactive = matches!(
            recovered.disposition,
            peritus_process::RecoveryDisposition::Terminal
                | peritus_process::RecoveryDisposition::AbsentUnobserved
        );
        self.receipts
            .as_mut()
            .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
            .mark_recovered_command_unknown(scope, ordinal, owner, owner_inactive)?;
        observation(
            &object(vec![
                (
                    "error",
                    Value::String(format!(
                        "the prior {tool_name} outcome remains unknown; the native command was not repeated"
                    )),
                ),
                ("ambiguous", Value::Bool(true)),
                ("owner_inactive", Value::Bool(owner_inactive)),
            ]),
            true,
        )
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
            "workspace_list"
                if result.get("path_kind").and_then(Value::as_str)
                    == Some("workspace-relative") =>
            {
                self.grounding.record_list(
                    string(arguments, "path").unwrap_or(""),
                    result.get("entries").and_then(Value::as_array).map_or(0, Vec::len),
                );
            }
            "workspace_search" => self.grounding.record_search(),
            "workspace_read" if result.get("reference_root").is_none() => {
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
        if name == "workspace_list"
            && result.get("path_kind").and_then(Value::as_str) == Some("workspace-relative")
        {
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
            ("sha256", Value::String(digest_hex(content.as_bytes()))),
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
        atomic_write(&path, replaced.as_bytes())?;
        Ok(object(vec![
            ("path", Value::String(relative.to_owned())),
            ("replacements", Value::from(if replace_all { occurrences } else { 1 })),
            ("bytes", Value::from(replaced.len())),
            ("sha256", Value::String(digest_hex(replaced.as_bytes()))),
        ]))
    }

    pub(super) fn remove(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        removal::remove(&self.root, &self.grounding, &self.ownership, arguments)
    }
}

fn routes_to_reference(root: &std::path::Path, arguments: &Value) -> bool {
    string(arguments, "path").is_some_and(|raw| {
        let path = std::path::Path::new(raw);
        path.is_absolute() && !path.starts_with(root)
    })
}

fn digest_hex(bytes: &[u8]) -> String {
    use core::fmt::Write as _;
    use sha2::Digest as _;

    let mut value = String::with_capacity(64);
    for byte in sha2::Sha256::digest(bytes) {
        let _ = write!(value, "{byte:02x}");
    }
    value
}
