# crates/orchestration/peritus-review/src/product — navigation

## Responsibility

Production-facing typed review and finding conservation.

## Files in this folder

| File | Role |
| --- | --- |
| [error.rs](error.rs) | Product review validation failures. |
| [finding.rs](finding.rs) | Typed product findings with policy-derived blocker status. |
| [ledger.rs](ledger.rs) | Append-preserving product finding ledger. |
| [mod.rs](mod.rs) | Production-facing typed review and finding conservation. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
