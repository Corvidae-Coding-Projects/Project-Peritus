//! Model validity/focus never proves independence: unknown conclusions retain broad dependencies.

use crate::local_context::{error, memory::LocalMemory};
use peritus_agent::DeveloperLoopError;
use peritus_context::{
    ContextNodeId,
    working::{WorkingEntryKind, WorkingEntryStatus},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Frontier {
    pub(super) sources: BTreeSet<u64>,
    pub(super) entries: usize,
}

pub(super) fn compile(memory: &LocalMemory) -> Result<Frontier, DeveloperLoopError> {
    let entries = memory
        .state
        .entries(memory.state.binding())
        .map_err(|_| error("decision frontier binding"))?;
    let lookup = entries.iter().map(|entry| (entry.id(), entry)).collect::<BTreeMap<_, _>>();
    let mut pending: Vec<ContextNodeId> = entries
        .iter()
        .filter(|entry| {
            entry.declared_status() != Some(WorkingEntryStatus::Resolved)
                && !matches!(
                    entry.status(),
                    WorkingEntryStatus::Resolved | WorkingEntryStatus::Superseded
                )
                && (entry.status() != WorkingEntryStatus::Stale
                    || matches!(
                        entry.kind(),
                        WorkingEntryKind::Plan | WorkingEntryKind::FailedApproach
                    ))
        })
        .map(peritus_context::working::WorkingEntry::id)
        .collect();
    let mut seen = BTreeSet::new();
    let mut sources =
        memory.transcript.pending.iter().map(|pending| pending.source).collect::<BTreeSet<_>>();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let entry = lookup.get(&id).ok_or_else(|| error("decision dependency entry missing"))?;
        sources.extend(
            entry.links().supports().iter().chain(entry.links().contradicts()).map(|id| id.get()),
        );
        pending.extend(entry.links().depends_on());
    }
    // These are priorities, never a proof that other supplied evidence is irrelevant.
    // The compiler retains all distinct tool evidence independently of model declarations.
    Ok(Frontier { sources, entries: seen.len() })
}
