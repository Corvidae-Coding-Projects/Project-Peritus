# crates/app/peritus-product-runner/src/control/goal — navigation

## Responsibility

Implementation pieces for `goal.rs`: Durable goal state, cumulative accounting, and safe-boundary admission.

## Files in this folder

| File | Role |
| --- | --- |
| [accounting.rs](accounting.rs) | Goal usage, admission, and settlement value types. |
| [evidence.rs](evidence.rs) | Exact output-search evidence bound to one graphical goal check operation. |
| [execution.rs](execution.rs) | Host-observed request, tool, runner, and graphical evidence transitions. |
| [lifecycle.rs](lifecycle.rs) | Goal construction, inspection, and user-controlled lifecycle transitions. |
| [validation.rs](validation.rs) | Goal admission boundaries and aggregate invariant validation. |

Owning module: [goal.rs](../goal.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
