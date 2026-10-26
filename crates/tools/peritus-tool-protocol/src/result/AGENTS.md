# crates/tools/peritus-tool-protocol/src/result — navigation

## Responsibility

Implementation pieces for `result.rs`: Truthful closed terminal result envelopes.

## Files in this folder

| File | Role |
| --- | --- |
| [accessors.rs](accessors.rs) | Read-only terminal-result projections and canonical encoding. |
| [wire.rs](wire.rs) | Deterministic terminal/failure envelope encoding. |

Owning module: [result.rs](../result.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
