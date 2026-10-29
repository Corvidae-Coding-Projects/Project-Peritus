# Inspection plan navigation

## Responsibility

Optional sequential read-only plans. Each child uses the ordinary executor and current host
admission. Typed host records preserve exact results without inventing native provider calls.
The first proposal upgrades the checkpoint reader guard to version 4 before child dispatch.

| File | Role |
| --- | --- |
| [mod.rs](mod.rs) | Dispatch, conservative rejection, and durable reader guard. |
| [contract.rs](contract.rs) | Closed versioned plan, reference, cursor, parent, and receipt records. |
| [schema.rs](schema.rs) | Published model-facing contracts. |
| [validation.rs](validation.rs) | Whole-graph validation and exact earlier-result reference resolution. |
| [admission.rs](admission.rs) | Current input, permission, cancellation, per-child accounting, and public activity. |
| [execution.rs](execution.rs) | Sequential dispatch and commit-before-next-child response paging. |
| [state.rs](state.rs) | Exact committed proposal/continuation reconstruction. |
| [records.rs](records.rs) | Canonical scoped host evidence framing and source archive access. |
| [read.rs](read.rs) | Permission-checked exact child JSON byte pages. |

Trace callers through [MemoryTools](../tools.rs), [source admission](../source_policy.rs),
[ingestion](../memory/ingestion.rs), and [recovery](../memory/recovery.rs). Disabling execution
retains v4 history and result retrieval. It never selects an older checkpoint.

Navigation: [Parent waypoint](../AGENTS.md).
