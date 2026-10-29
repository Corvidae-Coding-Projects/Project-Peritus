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
        let frontier = frontier::compile(memory)?;
        let bindings = bindings::capture(memory, &evidence)?;
        memory.decision_files = bindings.identity;
        let native_groups = if excluded {
            &groups[..0]
        } else {
            // Trailing host feedback/prose does not replace the current tool exchange.
            // Retain that complete pair and every later message in their original order.
            let start = groups
                .iter()
                .rposition(super::super::exchanges::Exchange::has_tool_calls)
                .unwrap_or_else(|| groups.len().saturating_sub(1));
            &groups[start..]
        };
        let mut native = Vec::new();
        for group in native_groups {
            native.extend(group.load(memory)?);
        }
        let mut native_sources: BTreeSet<u64> =
            native_groups.iter().flat_map(|group| group.sources.iter().copied()).collect();
        for id in native_sources.clone() {
            native_sources.insert(memory.decision_index.representative(id));
        }
        evidence.sort_by_key(|source| (!frontier.sources.contains(&source.source), source.source));
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
        }))
    }

    pub(super) fn omitted_entries(memory: &LocalMemory) -> Result<usize, DeveloperLoopError> {
        memory
            .state
            .entries(memory.state.binding())
            .map(<[_]>::len)
            .map_err(|_| error("decision working scope"))
    }
}
