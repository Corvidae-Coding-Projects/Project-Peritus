# crates/analysis/peritus-eval/src/accounting — navigation

## Responsibility

Complete rollout attempt, terminal, and resource accounting.

## Files in this folder

| File | Role |
| --- | --- |
| [ledger.rs](ledger.rs) | Canonical complete expected-rollout ledger with unique logical settlement. |
| [mod.rs](mod.rs) | Complete rollout attempt, terminal, and resource accounting. |
| [outcome.rs](outcome.rs) | Closed truthful rollout outcomes and retained attempts. |
| [record.rs](record.rs) | Complete immutable logical-terminal records. |
| [resource.rs](resource.rs) | Checked per-attempt cost, latency, usage, and execution observations. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
