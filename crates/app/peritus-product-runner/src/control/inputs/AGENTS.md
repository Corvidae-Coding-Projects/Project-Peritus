# crates/app/peritus-product-runner/src/control/inputs — navigation

## Responsibility

Implementation pieces for `inputs.rs`: Immutable input revisions and dependency-safe queue transitions, independent of transport.

## Files in this folder

| File | Role |
| --- | --- |
| [capture.rs](capture.rs) | Read-only request candidates. Capturing never incorporates an input or changes its state. |
| [transition.rs](transition.rs) | Pure successor planning. The caller publishes this successor and its receipt in one C0 append. |

Owning module: [inputs.rs](../inputs.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
