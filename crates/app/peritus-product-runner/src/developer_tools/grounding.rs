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

#[derive(Clone, Debug, Eq, PartialEq)]
struct TraversalPage {
    start: u64,
    end: u64,
    next: Option<String>,
    complete: bool,
}

#[derive(Clone, Debug)]
struct InspectionTraversal {
    subject: String,
    logical_start: u64,
    logical_end: u64,
    eligible: bool,
    root_empty: bool,
    initial: Option<TraversalPage>,
    pages: BTreeMap<String, TraversalPage>,
}

impl InspectionTraversal {
    fn merge(&mut self, other: &Self) {
        if self.subject != other.subject
            || self.logical_start != other.logical_start
            || self.logical_end != other.logical_end
        {
            return;
        }
        self.eligible &= other.eligible;
        self.root_empty |= other.root_empty;
        match (&self.initial, &other.initial) {
            (None, Some(page)) => self.initial = Some(page.clone()),
            (Some(left), Some(right)) if left != right => self.eligible = false,
            _ => {}
        }
        for (cursor, page) in &other.pages {
            match self.pages.get(cursor) {
                Some(retained) if retained != page => self.eligible = false,
                Some(_) => {}
                None => {
                    self.pages.insert(cursor.clone(), page.clone());
                }
            }
        }
    }

    fn complete(&self) -> bool {
        if !self.eligible {
            return false;
        }
        let Some(mut page) = self.initial.as_ref() else { return false };
        let mut expected = self.logical_start;
        let mut visited = BTreeSet::new();
        loop {
            if page.start != expected || page.end < page.start || page.end > self.logical_end {
                return false;
            }
            if page.complete {
                return page.next.is_none() && page.end == self.logical_end;
            }
            let Some(next) = page.next.as_ref() else { return false };
            if !visited.insert(next.clone()) {
                return false;
            }
            let Some(continued) = self.pages.get(next) else { return false };
            expected = page.end;
            page = continued;
        }
    }
}

#[derive(Clone, Default)]
struct InspectionTraversals {
    observations: BTreeMap<String, InspectionTraversal>,
}

impl InspectionTraversals {
    #[allow(clippy::too_many_arguments)]
    fn observe(
        &mut self,
        arguments: &Value,
        result: &Value,
        subject: String,
        logical_start: u64,
        logical_end: u64,
        page_start: u64,
        page_end: u64,
        eligible: bool,
        root_empty: bool,
    ) {
        let Some(observation) = result.get("observation").and_then(Value::as_str) else {
            return;
        };
        let cursor = arguments.get("cursor").and_then(Value::as_str);
        let replay = result.get("replay").and_then(Value::as_str);
        if cursor.is_some() && cursor != replay {
            return;
        }
        let next = result.get("next").and_then(Value::as_str).map(str::to_owned);
        let Some(complete) = result.get("complete").and_then(Value::as_bool) else {
            return;
        };
        if complete != next.is_none() {
            return;
        }
        let page = TraversalPage {
            start: page_start,
            end: page_end,
            next,
            complete,
        };
        let traversal = self
            .observations
            .entry(observation.to_owned())
            .or_insert_with(|| InspectionTraversal {
                subject: subject.clone(),
                logical_start,
                logical_end,
                eligible,
                root_empty,
                initial: None,
                pages: BTreeMap::new(),
            });
        if traversal.subject != subject
            || traversal.logical_start != logical_start
            || traversal.logical_end != logical_end
        {
            traversal.eligible = false;
            return;
        }
        traversal.eligible &= eligible;
        traversal.root_empty |= root_empty;
        if let Some(cursor) = cursor {
            match traversal.pages.get(cursor) {
                Some(retained) if retained != &page => traversal.eligible = false,
                Some(_) => {}
                None => {
                    traversal.pages.insert(cursor.to_owned(), page);
                }
            }
        } else {
            match &traversal.initial {
                Some(retained) if retained != &page => traversal.eligible = false,
                Some(_) => {}
                None => traversal.initial = Some(page),
            }
        }
    }

    fn merge(&mut self, other: &Self) {
        for (observation, incoming) in &other.observations {
            match self.observations.get_mut(observation) {
                Some(retained) => retained.merge(incoming),
                None => {
                    self.observations.insert(observation.clone(), incoming.clone());
                }
            }
        }
    }

    fn completed(&self) -> impl Iterator<Item = (&String, &InspectionTraversal)> {
        self.observations.iter().filter(|(_, traversal)| traversal.complete())
    }
}

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
    listings: InspectionTraversals,
    searches: InspectionTraversals,
    reads: InspectionTraversals,
    credited_listings: BTreeSet<String>,
    credited_searches: BTreeSet<String>,
    credited_reads: BTreeSet<String>,
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
            *self = Self {
                workspace_root: Some(root.to_owned()),
                ..Self::default()
            };
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
        self.listings.merge(&other.listings);
        self.searches.merge(&other.searches);
        self.reads.merge(&other.reads);
        self.credited_listings.extend(other.credited_listings.iter().cloned());
        self.credited_searches.extend(other.credited_searches.iter().cloned());
        self.credited_reads.extend(other.credited_reads.iter().cloned());
        self.refresh_inspection_credit();
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
            "workspace_list" | "workspace_search" | "workspace_read" => {
                self.record_inspection_result(call.name().as_str(), &arguments, &result);
            }
            "workspace_write" | "workspace_patch" | "workspace_remove" => {
                if let Some(path) = arguments.get("path").and_then(Value::as_str) {
                    self.record_mutation(path);
                }
            }
            _ => {}
        }
    }

    pub(in crate::developer_tools) fn record_inspection_result(
        &mut self,
        name: &str,
        arguments: &Value,
        result: &Value,
    ) -> bool {
        match name {
            "workspace_list"
                if result.get("path_kind").and_then(Value::as_str)
                    == Some("workspace-relative") =>
            {
                for path in result
                    .get("entries")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| entry.get("path").and_then(Value::as_str))
                {
                    self.record_listed_path(path);
                }
                let subject = arguments
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                if result.get("observation").is_some() {
                    if let (Some(total), Some(start), Some(count)) = (
                        decimal_field(result, "total_observed_entries"),
                        decimal_field(result, "start"),
                        decimal_field(result, "page_observed_entries"),
                    ) && let Some(end) = start.checked_add(count)
                    {
                        self.listings.observe(
                            arguments,
                            result,
                            subject,
                            0,
                            total,
                            start,
                            end,
                            true,
                            result.get("exact_empty").and_then(Value::as_bool) == Some(true),
                        );
                        self.refresh_inspection_credit();
                    }
                } else if result.get("truncated").and_then(Value::as_bool) == Some(false) {
                    let entries = result.get("entries").and_then(Value::as_array);
                    let observed = result
                        .get("exact_empty")
                        .and_then(Value::as_bool)
                        .map_or_else(
                            || entries.map_or(0, Vec::len),
                            |empty| usize::from(!empty),
                        );
                    self.record_list(&subject, observed);
                }
            }
            "workspace_search" => {
                let subject = arguments
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                if result.get("observation").is_some() {
                    if let (Some(total), Some(start), Some(count)) = (
                        decimal_field(result, "total_records"),
                        decimal_field(result, "start"),
                        decimal_field(result, "page_returned_records"),
                    ) && let Some(end) = start.checked_add(count)
                    {
                        self.searches.observe(
                            arguments,
                            result,
                            subject,
                            0,
                            total,
                            start,
                            end,
                            result
                                .get("coverage_complete")
                                .and_then(Value::as_bool)
                                == Some(true),
                            false,
                        );
                        self.refresh_inspection_credit();
                    }
                } else if result.get("truncated").and_then(Value::as_bool) == Some(false) {
                    self.record_search();
                }
            }
            "workspace_read" if result.get("reference_root").is_none() => {
                let Some(path) = arguments.get("path").and_then(Value::as_str) else {
                    return false;
                };
                if result.get("observation").is_some() {
                    if let (Some(selection_start), Some(selection_end), Some(start), Some(end)) = (
                        decimal_field(result, "selection_start_byte"),
                        decimal_field(result, "selection_end_byte"),
                        decimal_field(result, "returned_start_byte"),
                        decimal_field(result, "returned_end_byte"),
                    ) {
                        self.reads.observe(
                            arguments,
                            result,
                            path.to_owned(),
                            selection_start,
                            selection_end,
                            start,
                            end,
                            result
                                .get("coverage_complete")
                                .and_then(Value::as_bool)
                                .unwrap_or(true),
                            false,
                        );
                        self.refresh_inspection_credit();
                    }
                } else if legacy_read_complete(arguments, result) {
                    self.record_read(path);
                }
                return self.read_paths.contains(&PathBuf::from(path));
            }
            _ => {}
        }
        false
    }

    fn refresh_inspection_credit(&mut self) {
        let listings = self
            .listings
            .completed()
            .map(|(observation, traversal)| {
                (
                    observation.clone(),
                    traversal.subject.clone(),
                    traversal.root_empty,
                )
            })
            .collect::<Vec<_>>();
        for (observation, subject, root_empty) in listings {
            if self.credited_listings.insert(observation) {
                self.list_calls = self.list_calls.saturating_add(1);
                if root_empty && (subject.is_empty() || subject == ".") {
                    self.root_observed_empty = true;
                }
            }
        }
        let searches = self
            .searches
            .completed()
            .map(|(observation, _)| observation.clone())
            .collect::<Vec<_>>();
        for observation in searches {
            if self.credited_searches.insert(observation) {
                self.search_calls = self.search_calls.saturating_add(1);
            }
        }
        let reads = self
            .reads
            .completed()
            .map(|(observation, traversal)| {
                (observation.clone(), traversal.subject.clone())
            })
            .collect::<Vec<_>>();
        for (observation, path) in reads {
            if self.credited_reads.insert(observation) {
                self.read_paths.insert(PathBuf::from(path));
            }
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

fn decimal_field(value: &Value, name: &str) -> Option<u64> {
    value
        .get(name)
        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
}

fn legacy_read_complete(arguments: &Value, result: &Value) -> bool {
    let Some(content) = result.get("content").and_then(Value::as_str) else {
        return false;
    };
    if content.contains("[output truncated]") {
        return false;
    }
    if arguments.get("start_line").is_some() || arguments.get("end_line").is_some() {
        return true;
    }
    if result.get("bytes").and_then(Value::as_u64) == Some(0) {
        return content.is_empty();
    }
    content
        .lines()
        .next_back()
        .and_then(|line| line.split_once(':'))
        .and_then(|(line, _)| line.parse::<u64>().ok())
        .is_some_and(|line| line < 500)
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
