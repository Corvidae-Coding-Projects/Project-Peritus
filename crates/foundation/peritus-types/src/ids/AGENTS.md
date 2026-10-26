# crates/foundation/peritus-types/src/ids — navigation

## Responsibility

Nominal identifier types grouped by domain responsibility.

## Files in this folder

| File | Role |
| --- | --- |
| [base.rs](base.rs) | Shared verified representation used behind each nominal identifier. |
| [budget.rs](budget.rs) | Identifiers for immutable budget accounts and reservation lineages. |
| [evolution.rs](evolution.rs) | Identifiers for isolated harness evaluation and evolution campaigns. |
| [execution.rs](execution.rs) | Identifiers for executable work, actors, policy, and workspaces. |
| [lifecycle.rs](lifecycle.rs) | Identifiers for project and execution-lifecycle aggregates. |
| [mod.rs](mod.rs) | Nominal identifier types grouped by domain responsibility. |
| [records.rs](records.rs) | Identifiers for journal, process, artifact, and evidence records. |
| [review.rs](review.rs) | Identifiers for deterministic gates, adversarial review, and approvals. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
