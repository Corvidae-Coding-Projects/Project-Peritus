# crates/app/testing/peritus-external-benchmarks/src/trace — navigation

## Responsibility

Projection of the durable D0 trace into external benchmark evidence.

## Files in this folder

| File | Role |
| --- | --- |
| [bounded.rs](bounded.rs) | Bounded external projections of values retained exactly in the native trace. |
| [frames.rs](frames.rs) | Checked decoding for the product runner's length-framed developer trace. |
| [harnessbench.rs](harnessbench.rs) | Atomic `HarnessBench` usage-proxy publication. |
| [metadata.rs](metadata.rs) | Validation for trace metadata that does not alter projected conversation history. |
| [mod.rs](mod.rs) | Projection of the durable D0 trace into external benchmark evidence. |
| [projection.rs](projection.rs) | Normalized response, tool, and usage reconstruction from D0 trace frames. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
