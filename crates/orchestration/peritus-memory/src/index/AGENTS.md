# crates/orchestration/peritus-memory/src/index — navigation

## Responsibility

Implementation pieces for `index.rs`: Canonical replay-derived memory index.

## Files in this folder

| File | Role |
| --- | --- |
| [canonical.rs](canonical.rs) | Versioned canonical bytes and real SHA-256 for rebuilt indexes. |
| [rebuild.rs](rebuild.rs) | Canonical replay, tombstone dominance, and posting-list construction. |
| [types.rs](types.rs) | Immutable canonical index views and posting lists. |

Owning module: [index.rs](../index.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
