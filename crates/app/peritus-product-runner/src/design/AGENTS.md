# crates/app/peritus-product-runner/src/design — navigation

## Responsibility

Implementation pieces for `design.rs`: Task plans handed off after one startup grounding check,
and host-validated accepted-design reuse. Source questions left open by the plan belong to the
next implementation slice; the designer cannot start another discovery loop.

## Files in this folder

| File | Role |
| --- | --- |
| [reuse.rs](reuse.rs) | Exact request/design receipts and fresh ready-design handoff; task or draft changes invalidate reuse. |
| [artifact.rs](artifact.rs) | Deterministic detailed designs for explicit generated-artifact workspaces. |

Owning module: [design.rs](../design.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
