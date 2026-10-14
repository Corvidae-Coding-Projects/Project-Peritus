# crates/state/peritus-journal/src/sqlite/history — navigation

## Responsibility

Implementation pieces for `history.rs`: Lossless, bounded, content-addressed immutable state history.

## Files in this folder

| File | Role |
| --- | --- |
| [node.rs](node.rs) | Canonical fixed-leaf, fixed-fanout node encoding. Logical state hashes remain unchanged. |

Owning module: [history.rs](../history.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
