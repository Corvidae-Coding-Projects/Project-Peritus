//! One current dependency/admission snapshot for a decision view; no authority is cached.

use super::{
    bindings::{self, Bindings},
    frontier::{self, Frontier},
    index::Evidence,
};
use crate::local_context::{error, memory::LocalMemory};
use peritus_agent::{DeveloperContextEstimate, DeveloperLoopError};
use peritus_model_protocol::{Message, ToolDefinition};
use std::collections::BTreeSet;

pub(super) struct Plan {
    pub(super) native: Vec<Message>,
    pub(super) native_sources: BTreeSet<u64>,
    pub(super) native_groups: usize,
    pub(super) evidence: Vec<Evidence>,
    pub(super) readable: BTreeSet<u64>,
    pub(super) excluded: bool,
    pub(super) frontier: Frontier,
    pub(super) bindings: Bindings,
    pub(super) uncompacted: u64,
    active_files: BTreeSet<String>,
    active_after: u64,
}

impl Plan {
    /// None retains the existing selector for an unproven native replay dependency.
    pub(super) fn prepare(
        memory: &mut LocalMemory,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Option<Self>, DeveloperLoopError> {
        let mut index = std::mem::take(&mut memory.decision_index);
        let indexed = index.refresh(memory);
        memory.decision_index = index;
        indexed?;
        let groups = super::super::exchanges::groups(memory)?;
        let uncompacted = groups
            .iter()
            .fold(DeveloperContextEstimate::request(messages, tools), |sum, group| {
                sum.with(group.estimate)
            })
            .tokens();
        let read_allowed = memory.conversation.as_ref().is_some_and(|view| {
            view.effective_permissions().allows(crate::control::PermissionCapability::Read)
        });
        let mut evidence = memory.decision_index.evidence();
        let mut readable = BTreeSet::new();
        for source in &evidence {
            if read_allowed && crate::local_context::source_policy::allowed(memory, source.source)?
            {
                readable.insert(source.source);
            }
        }
        let excluded = !read_allowed || readable.len() != evidence.len();
        if !excluded && groups.iter().any(super::super::exchanges::Exchange::requires_native_replay)
        {
            return Ok(None);
        }
        let mut frontier = frontier::compile(memory)?;
        frontier.sources =
            frontier.sources.iter().map(|id| memory.decision_index.representative(*id)).collect();
        let bindings = bindings::capture(memory, &evidence)?;
        memory.decision_files = bindings.identity;
        let native_groups = if excluded {
            Vec::new()
        } else {
            // Trailing host feedback/prose does not replace the current tool exchange.
            // Retain that complete pair and every later message in their original order.
            let start = groups
                .iter()
                .rposition(super::super::exchanges::Exchange::has_tool_calls)
                .unwrap_or_else(|| groups.len().saturating_sub(1));
            let mut chosen = (start..groups.len()).collect::<BTreeSet<_>>();
            // An already-visible receipt promises a body in the next view. Retain the
            // nearest real page, not an ever-growing chain of earlier reuse receipts.
            for (key, body) in groups[start..].iter().flat_map(|group| &group.recovery_pages) {
                if !body
                    && let Some(index) = groups[..start]
                        .iter()
                        .rposition(|group| group.recovery_pages.contains(&(*key, true)))
                {
                    chosen.insert(index);
                }
            }
            chosen.into_iter().map(|index| &groups[index]).collect()
        };
        let mut native = Vec::new();
        for group in &native_groups {
            native.extend(group.load(memory)?);
        }
        let mut native_sources: BTreeSet<u64> =
            native_groups.iter().flat_map(|group| group.sources.iter().copied()).collect();
        for id in native_sources.clone() {
            native_sources.insert(memory.decision_index.representative(id));
        }
        let (active_files, active_after) = active_scope(memory, &evidence)?;
        evidence.sort_by_key(|source| {
            (
                !current(source, &bindings, &active_files, active_after),
                !frontier.sources.contains(&source.source),
                std::cmp::Reverse(if current(source, &bindings, &active_files, active_after) {
                    source.body_bytes
                } else {
                    0
                }),
                std::cmp::Reverse(source.last_seen),
            )
        });
        Ok(Some(Self {
            native,
            native_sources,
            native_groups: native_groups.len(),
            evidence,
            readable,
            excluded,
            frontier,
            bindings,
            uncompacted,
            active_files,
            active_after,
        }))
    }

    pub(super) fn current(&self, source: &Evidence) -> bool {
        current(source, &self.bindings, &self.active_files, self.active_after)
    }

    pub(super) fn omitted_entries(memory: &LocalMemory) -> Result<usize, DeveloperLoopError> {
        memory
            .state
            .entries(memory.state.binding())
            .map(<[_]>::len)
            .map_err(|_| error("decision working scope"))
    }
}

fn current(source: &Evidence, bindings: &Bindings, files: &BTreeSet<String>, after: u64) -> bool {
    source.page.is_some()
        && source.has_current_content(bindings)
        && (source.last_seen >= after
            || source.file.as_ref().is_some_and(|path| files.contains(path)))
}

fn active_scope(
    memory: &LocalMemory,
    evidence: &[Evidence],
) -> Result<(BTreeSet<String>, u64), DeveloperLoopError> {
    let mutations = evidence
        .iter()
        .filter(|source| {
            source.file.is_some()
                && matches!(
                    source.tool.as_str(),
                    "workspace_patch" | "workspace_write" | "workspace_remove"
                )
        })
        .collect::<Vec<_>>();
    let Some(latest) = mutations.iter().max_by_key(|source| source.source) else {
        let after = memory
            .sources
            .iter()
            .find(|source| source.invocation == memory.transcript.invocation)
            .map_or(u64::MAX, |source| source.sequence);
        return Ok((BTreeSet::new(), after));
    };
    let invocation = memory.archived(latest.source)?.invocation;
    let mut files = BTreeSet::new();
    for source in &mutations {
        if memory.archived(source.source)?.invocation == invocation
            && let Some(path) = &source.file
        {
            files.insert(path.clone());
        }
    }
    Ok((files, latest.source))
}
