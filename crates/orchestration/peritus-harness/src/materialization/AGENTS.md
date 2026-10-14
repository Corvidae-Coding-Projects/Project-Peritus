# crates/orchestration/peritus-harness/src/materialization — navigation

## Responsibility

Pure exact materialization planning and the narrow C1 executor.

## Files in this folder

| File | Role |
| --- | --- |
| [error.rs](error.rs) | Typed materialization failures and recovery guidance. |
| [executor.rs](executor.rs) | Exact artifact-to-`PatchSet` construction and C1 execution. |
| [failure.rs](failure.rs) | Durable bounded diagnostics for unsuccessful materializations. |
| [mod.rs](mod.rs) | Pure exact materialization planning and the narrow C1 executor. |
| [plan_codec.rs](plan_codec.rs) | Canonical materialization plan encoding and decoding. |
| [plan_model.rs](plan_model.rs) | Durable inert materialization plan value types. |
| [planner.rs](planner.rs) | Pure materialization plan construction. |
| [receipt.rs](receipt.rs) | Exact successful materialization receipt model and C1 validation. |
| [receipt_codec.rs](receipt_codec.rs) | Canonical materialization receipt encoding and checked reconstruction. |
| [snapshot.rs](snapshot.rs) | Exact C1 snapshot and observed target values used by materialization plans. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
