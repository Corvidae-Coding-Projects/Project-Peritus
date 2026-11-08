# Capability delivery

The ordinary product runner owns this durable construction/verification workflow. It does not
grant tool authority or replace the daemon's run ownership and settlement.

| File | Responsibility |
| --- | --- |
| `contract.rs` | Typed capability proposals, draft handoffs and independent assessments; source/mapping/dependency validation. |
| `store.rs` | Run/workspace/design-bound state loading, atomic sidecar publication and recovery of an exact admitted design after interruption. |
| `workflow.rs` | Dependency-ready selection, draft/debt tracking, rotation/decomposition, prompts and per-capability evidence admission. |
| `tests.rs` | Durable scheduling, missing-proof, stale-proof and scope-preservation regression replays. |

The caller is [turn.rs](../turn.rs) for construction and [execution](../execution/AGENTS.md) for
qualification and recovery. [Command evidence](../developer_tools/evidence.rs) supplies actual
successful command references bound to source observations. Read the callers before changing the
definition of drafted, reviewed, or qualified; these states are deliberately distinct.
