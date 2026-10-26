# crates/model/peritus-provider-openai/src/runtime/output — navigation

## Responsibility

Implementation pieces for `output.rs`: Fail-closed decoding of the bounded Codex JSONL turn transcript.

## Files in this folder

| File | Role |
| --- | --- |
| [failure.rs](failure.rs) | Redaction-safe classification of official Codex runtime failure families. |
| [selection.rs](selection.rs) | Selects the one authoritative host-visible result from a completed runtime transcript. |

Owning module: [output.rs](../output.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
