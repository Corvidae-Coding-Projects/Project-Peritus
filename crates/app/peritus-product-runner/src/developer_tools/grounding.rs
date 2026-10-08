//! Deterministic evidence that a model inspected the managed repository.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::PathBuf,
};

use peritus_model_protocol::{CanonicalJson, CompletedToolCall, ContentBlock, Message, ToolCallId};
use serde_json::Value;

use super::executor::{
    inspection_progress::{InspectionLedger, LedgerObservation, ProgressDelta},
    sources::RequestSourceProgress,
};

#[derive(Clone, Default)]
pub struct GroundingEvidence {
    list_calls: u32,
    search_calls: u32,
    listed_paths: BTreeSet<PathBuf>,
    read_paths: BTreeSet<PathBuf>,
    mutation_paths: BTreeSet<PathBuf>,
    root_observed_empty: bool,
    workspace_root: Option<PathBuf>,
    pub(in crate::developer_tools) request_sources: RequestSourceProgress,
    progress: InspectionLedger,
}

impl GroundingEvidence {
    pub(crate) fn for_workspace(root: &std::path::Path) -> Self {
        Self { workspace_root: Some(root.to_owned()), ..Self::default() }
    }

    pub(crate) fn clear_repository_evidence(&mut self) {
        *self = Self {
            workspace_root: self.workspace_root.clone(),
            request_sources: std::mem::take(&mut self.request_sources),
            ..Self::default()
        };
    }

    pub(in crate::developer_tools) fn is_for_workspace(&self, root: &std::path::Path) -> bool {
        self.workspace_root.as_deref() == Some(root)
    }

    pub(in crate::developer_tools) fn bind_workspace(&mut self, root: &std::path::Path) {
        if !self.is_for_workspace(root) {
            self.request_sources = RequestSourceProgress::default();
            self.progress = InspectionLedger::default();
            self.workspace_root = Some(root.to_owned());
        }
    }

    pub(in crate::developer_tools) fn bind_progress(
        &mut self,
        revision: u64,
        request_sources: [u8; 32],
    ) {
        self.progress.bind(revision, request_sources);
    }

    pub(in crate::developer_tools) fn progress_binding(&self) -> Option<(u64, [u8; 32])> {
        self.progress.binding()
    }

    pub(crate) fn merge(&mut self, other: &Self) {
        self.list_calls = self.list_calls.max(other.list_calls);
        self.search_calls = self.search_calls.max(other.search_calls);
        self.listed_paths.extend(other.listed_paths.iter().cloned());
        self.read_paths.extend(other.read_paths.iter().cloned());
        self.mutation_paths.extend(other.mutation_paths.iter().cloned());
        self.root_observed_empty |= other.root_observed_empty;
        self.progress.merge(&other.progress);
        if self.workspace_root == other.workspace_root {
            self.request_sources.merge(&other.request_sources);
        }
    }

    pub(crate) fn recover_model_context(&mut self, messages: &[Message]) {
        let mut calls = BTreeMap::<ToolCallId, CompletedToolCall>::new();
        let mut recovered = Self {
            workspace_root: self.workspace_root.clone(),
            ..Self::default()
        };
        for message in messages {
            for block in message.content() {
                match block {
                    ContentBlock::ToolCall(call) => {
                        calls.insert(call.id().clone(), call.clone());
                    }
                    ContentBlock::ToolResult(result) if !result.is_error() => {
                        if let Some(call) = calls.remove(result.call_id()) {
                            recovered.record_completed(&call, result.output());
                        }
                    }
                    _ => {}
                }
            }
        }
        self.merge(&recovered);
    }

    pub(crate) fn record_completed(
        &mut self,
        call: &CompletedToolCall,
        output: &CanonicalJson,
    ) {
        let Ok(arguments) = serde_json::from_slice::<Value>(call.arguments().canonical_bytes())
        else {
            return;
        };
        let Ok(result) = serde_json::from_slice::<Value>(output.canonical_bytes()) else {
            return;
        };
        let mutation_boundary = matches!(
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
        );
        let delta = ProgressDelta::from_observation(
            call.name().as_str(),
            &arguments,
            &result,
            true,
            false,
            mutation_boundary,
        );
        let _ = self
            .progress
            .observe(call.name().as_str(), &arguments, &result, delta);
        match call.name().as_str() {
            "request_sources" | "request_source_read" => {
                if let Some(root) = &self.workspace_root {
                    self.request_sources.recover_completed(
                        root, call.name().as_str(), &arguments, &result,
                    );
                }
            }
            "workspace_list"
                if result.get("path_kind").and_then(Value::as_str)
                    == Some("workspace-relative") =>
            {
                let path = arguments.get("path").and_then(Value::as_str).unwrap_or("");
                let entries = result.get("entries").and_then(Value::as_array);
                let observed = result
                    .get("exact_empty")
                    .and_then(Value::as_bool)
                    .map_or_else(|| entries.map_or(0, Vec::len), |empty| usize::from(!empty));
                self.record_list(path, observed);
                for path in entries
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| entry.get("path").and_then(Value::as_str))
                {
                    self.record_listed_path(path);
                }
            }
            "workspace_search" => self.record_search(),
            "workspace_read" if result.get("reference_root").is_none() => {
                if let Some(path) = arguments.get("path").and_then(Value::as_str) {
                    self.record_read(path);
                }
            }
            "workspace_write" | "workspace_patch" | "workspace_remove" => {
                if let Some(path) = arguments.get("path").and_then(Value::as_str) {
                    self.record_mutation(path);
                }
            }
            _ => {}
        }
    }

    pub(in crate::developer_tools) fn record_progress(
        &mut self,
        name: &str,
        arguments: &Value,
        result: &Value,
        delta: ProgressDelta,
    ) -> LedgerObservation {
        self.progress.observe(name, arguments, result, delta)
    }

    pub fn record_list(&mut self, path: &str, entries: usize) {
        self.list_calls = self.list_calls.saturating_add(1);
        if (path.is_empty() || path.as_bytes() == b".") && entries == 0 {
            self.root_observed_empty = true;
        }
    }

    pub fn required_tool_name(&self) -> Option<&'static str> {
        if self.list_calls == 0 {
            Some("workspace_list")
        } else if self.read_paths.is_empty() && !self.root_observed_empty {
            Some("workspace_read")
        } else {
            None
        }
    }

    pub const fn record_search(&mut self) {
        self.search_calls = self.search_calls.saturating_add(1);
    }

    pub fn record_listed_path(&mut self, path: &str) {
        self.listed_paths.insert(PathBuf::from(path));
    }

    pub fn record_read(&mut self, path: &str) {
        self.read_paths.insert(PathBuf::from(path));
    }

    pub fn record_mutation(&mut self, path: &str) {
        self.mutation_paths.insert(PathBuf::from(path));
    }

    pub fn ensure_mutation_allowed(&self, path: &str, exists: bool) -> Result<(), String> {
        self.validate().map_err(str::to_owned)?;
        if exists && !self.read_paths.contains(&PathBuf::from(path)) {
            return Err(format!("read the existing target before mutating it: {path}"));
        }
        Ok(())
    }

    pub fn ensure_empty_directory_removal_allowed(&self, path: &str) -> Result<(), String> {
        self.validate().map_err(str::to_owned)?;
        if !self.listed_paths.contains(&PathBuf::from(path)) {
            return Err(format!("list the empty directory before removing it: {path}"));
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.list_calls == 0 {
            return Err("repository grounding requires a successful workspace listing");
        }
        if self.read_paths.is_empty() && !self.root_observed_empty {
            return Err("repository grounding requires reading an observed repository file");
        }
        Ok(())
    }

    pub fn markdown(&self) -> String {
        let mut text = String::from("\n## Repository grounding evidence\n\n");
        let _ = write!(
            text,
            "- Workspace listings: {}\n- Targeted searches: {}\n",
            self.list_calls, self.search_calls,
        );
        if self.root_observed_empty {
            text.push_str("- The workspace root was observed to be empty.\n");
        }
        if !self.read_paths.is_empty() {
            text.push_str("- Files read:\n");
            for path in &self.read_paths {
                let _ = write!(text, "  - `{}`", path.display());
                text.push('\n');
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_target_requires_repository_and_target_observation() {
        let mut evidence = GroundingEvidence::default();
        assert_eq!(evidence.required_tool_name(), Some("workspace_list"));
        assert!(evidence.ensure_mutation_allowed("src/lib.rs", true).is_err());
        evidence.record_list("", 2);
        assert_eq!(evidence.required_tool_name(), Some("workspace_read"));
        evidence.record_read("Cargo.toml");
        assert_eq!(evidence.required_tool_name(), None);
        assert!(evidence.ensure_mutation_allowed("src/lib.rs", true).is_err());
        evidence.record_read("src/lib.rs");
        assert!(evidence.ensure_mutation_allowed("src/lib.rs", true).is_ok());
    }

    #[test]
    fn observed_empty_repository_allows_new_files() {
        let mut evidence = GroundingEvidence::default();
        evidence.record_list(".", 0);
        assert_eq!(evidence.required_tool_name(), None);
        assert!(evidence.ensure_mutation_allowed("src/main.rs", false).is_ok());
    }

    #[test]
    fn empty_directory_removal_requires_exact_listing_evidence() {
        let mut evidence = GroundingEvidence::default();
        evidence.record_list("", 2);
        evidence.record_read("Cargo.toml");
        assert!(evidence.ensure_empty_directory_removal_allowed("out/tmp").is_err());
        evidence.record_listed_path("out/tmp");
        assert!(evidence.ensure_empty_directory_removal_allowed("out/tmp").is_ok());
    }
}
