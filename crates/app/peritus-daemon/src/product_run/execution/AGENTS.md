# crates/app/peritus-daemon/src/product_run/execution — navigation

## Responsibility

Implementation pieces for `execution.rs`: Active product-run task ownership and terminal projection.

## Files in this folder

| File | Role |
| --- | --- |
| [baseline.rs](baseline.rs) | Embed validated task preimages before publishing any managed candidate handoff. |
| [goal.rs](goal.rs) | Exact runner terminal settlement projected into the cumulative goal ledger. |
| [handoff.rs](handoff.rs) | Terminal user-facing summaries and explicit handoff failure projection. |
| [launch.rs](launch.rs) | Owned asynchronous run launch and provider/runtime composition. |
| [runtime.rs](runtime.rs) | Command runtime construction for exact managed and direct-folder capabilities. |

Owning module: [execution.rs](../execution.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
