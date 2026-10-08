//! Effect receipts, delivery progress, and bounded filesystem mutations.

use super::{
    CompletedToolCall, DeveloperLoopError, DeveloperToolExecutor, DeveloperToolObservation,
    ReceiptDecision, Value, WorkspaceDeveloperTools, atomic_write, checked, fs,
    literal_patch::apply_literal_patch, object, removal, required_string, string, tool,
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
                let accepted = value.get("error").is_none();
                self.finish_observation(call, arguments, &value, is_error, accepted, false)
                    .map(Some)
            }
            ReceiptDecision::RecoverCheckpoint { value, is_error } => {
                self.recover_checkpoint(call.name().as_str(), arguments, &value)?;
                self.receipts
                    .as_mut()
                    .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
                    .finalize()?;
                let accepted = value.get("error").is_none();
                self.finish_observation(call, arguments, &value, is_error, accepted, false)
                    .map(Some)
            }
            ReceiptDecision::Refuse { detail, ambiguous } => {
                let value = object(vec![
                    ("error", Value::String(detail)),
                    ("ambiguous", Value::Bool(ambiguous)),
                ]);
                self.finish_observation(call, arguments, &value, true, false, false)
                    .map(Some)
            }
            ReceiptDecision::Execute => Err(tool("receipt replay returned an execute decision")),
        }
    }

    pub(super) fn dispatch_tool(
        &mut self,
        call: &CompletedToolCall,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        use super::inspection;
        use crate::developer_tools::reference;
        match call.name().as_str() {
            "request_sources" => self.request_sources(arguments),
            "request_source_read" => self.request_source_read(arguments),
            "context_sources" => self.context_sources(arguments),
            "context_source_read" => self.context_source_read(arguments),
            "workspace_list" if routes_to_reference(&self.root, arguments) => {
                reference::list(&self.references, arguments)
            }
            "workspace_list" => {
                inspection::list(
                    &self.root,
                    arguments,
                    self.resources,
                    &self.access_policy,
                    self.directory_listings
                        .as_ref()
                        .ok_or_else(|| tool("workspace listing owner is unavailable"))?,
                )
            }
            "workspace_search" => inspection::search(&self.root, arguments, &self.access_policy),
            "workspace_read" if routes_to_reference(&self.root, arguments) => {
                reference::read(&self.references, arguments)
            }
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
                let accepted = value.get("error").is_none();
                self.finish_observation(call, arguments, &value, is_error, accepted, false)
                    .map(Some)
            }
            ReceiptDecision::RecoverCheckpoint { .. } => {
                Err(tool("checkpoint recovery must occur before effect preflight"))
            }
            ReceiptDecision::Refuse { detail, ambiguous } => {
                let value = object(vec![
                    ("error", Value::String(detail)),
                    ("ambiguous", Value::Bool(ambiguous)),
                ]);
                self.finish_observation(call, arguments, &value, true, false, false)
                    .map(Some)
            }
        }
    }

    pub(super) fn observe_progress(
        &mut self,
        name: &str,
        arguments: &Value,
        result: &Value,
        accepted: bool,
        required_before: Option<&str>,
        mutation_boundary: bool,
    ) {
        let required_after =
            DeveloperToolExecutor::required_tool_name(self).map(str::to_owned);
        let discharged = accepted
            && required_before.is_some_and(|required| {
                required_after.as_deref() != Some(required)
            });
        let stalled = required_before.filter(|required| {
            required_after.as_deref() == Some(*required) && *required != name
        });
        let delta = super::inspection_progress::ProgressDelta::from_observation(
            name,
            arguments,
            result,
            accepted,
            discharged,
            mutation_boundary,
        );
        let observation = self
            .grounding
            .record_progress(name, arguments, result, delta);
        self.inspection_progress
            .observe_progress(name, observation, stalled);
        #[cfg(test)]
        if delta.candidate_advanced() {
            self.progress_nudges = 0;
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
                    result
                        .get("exact_empty")
                        .and_then(Value::as_bool)
                        .map_or(1, |empty| usize::from(!empty)),
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
        if self.file_content_matches(relative, content.as_bytes())? {
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
        let applied = apply_literal_patch(&self.root, relative, &path, old, new, replace_all)?;
        Ok(object(vec![
            ("path", Value::String(relative.to_owned())),
            ("replacements", Value::from(applied.replacements())),
            ("bytes", Value::from(applied.bytes())),
            ("sha256", Value::String(applied.digest_hex())),
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
