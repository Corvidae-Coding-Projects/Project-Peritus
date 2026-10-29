//! Shared non-authoritative state vocabulary for inspection and optional local inference.

use super::{LocalMemory, hex, source_handle};
use peritus_context::working::{WorkingEntry, WorkingEntryKind, WorkingEntryStatus};
use serde_json::Value;

pub(in crate::local_context) fn entry_view(memory: &LocalMemory, entry: &WorkingEntry) -> Value {
    let kind = match entry.kind() {
        WorkingEntryKind::Observation => "observation",
        WorkingEntryKind::Assertion => "assertion",
        WorkingEntryKind::Hypothesis => "hypothesis",
        WorkingEntryKind::Decision => "decision",
        WorkingEntryKind::FailedApproach => "failed_approach",
        WorkingEntryKind::Plan => "plan",
        WorkingEntryKind::Glossary => "glossary",
    };
    let status = match entry.status() {
        WorkingEntryStatus::Open => "open",
        WorkingEntryStatus::Contradicted => "contradicted",
        WorkingEntryStatus::Resolved => "resolved",
        WorkingEntryStatus::Stale => "stale",
        WorkingEntryStatus::Superseded => "superseded",
    };
    let mut value = Value::from_iter([
        ("id", Value::from(format!("entry:{}", hex(entry.id().as_bytes())))),
        ("kind", Value::from(kind)),
        ("status", Value::from(status)),
        ("text", Value::from(String::from_utf8_lossy(entry.content().bytes()))),
        (
            "supports",
            Value::from(
                entry
                    .links()
                    .supports()
                    .iter()
                    .map(|id| source_handle(memory, id.get()))
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "contradicts",
            Value::from(
                entry
                    .links()
                    .contradicts()
                    .iter()
                    .map(|id| source_handle(memory, id.get()))
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "depends_on",
            Value::from(
                entry
                    .links()
                    .depends_on()
                    .iter()
                    .map(|id| format!("entry:{}", hex(id.as_bytes())))
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "supersedes",
            Value::from(entry.supersedes().map(|id| format!("entry:{}", hex(id.as_bytes())))),
        ),
        ("authority", Value::from("none")),
    ]);
    if entry.host_binding().is_some()
        && memory.checkpoint_schema >= super::super::record::DEPENDENCY_CHECKPOINT_SCHEMA_VERSION
    {
        value["declared_status"] = entry.declared_status().map_or(Value::Null, |status| {
            Value::from(match status {
                WorkingEntryStatus::Open => "open",
                WorkingEntryStatus::Contradicted => "contradicted",
                WorkingEntryStatus::Resolved => "resolved",
                WorkingEntryStatus::Stale | WorkingEntryStatus::Superseded => "unknown",
            })
        });
    }
    value
}
