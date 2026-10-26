# crates/orchestration/peritus-obligations/src/ledger — navigation

## Responsibility

Implementation pieces for `ledger.rs`: Deterministic extraction of typed requirements from exact public source spans.

## Files in this folder

| File | Role |
| --- | --- |
| [extraction.rs](extraction.rs) | Exact source-span copying after verified extraction admission. |
| [validation.rs](validation.rs) | Extraction admission in production error order. |

Owning module: [ledger.rs](../ledger.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
