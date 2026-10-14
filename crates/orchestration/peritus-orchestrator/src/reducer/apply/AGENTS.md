# crates/orchestration/peritus-orchestrator/src/reducer/apply — navigation

## Responsibility

Implementation pieces for `apply.rs`: Phase-local dispatch for already fenced E0 commands.

## Files in this folder

| File | Role |
| --- | --- |
| [acceptance.rs](acceptance.rs) | Candidate advancement, B2/B0 acceptance, pause, cancellation, and settlement. |
| [directives.rs](directives.rs) | Commit-before-effect directive publication and acknowledgement. |
| [role_cycle.rs](role_cycle.rs) | Exact D3 activation and writer/gate/review/fixer observation transitions. |

Owning module: [apply.rs](../apply.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
