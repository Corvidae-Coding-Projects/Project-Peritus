# crates/analysis/peritus-eval/src/plan — navigation

## Responsibility

Deterministic paired rollout planning and D3 work binding.

## Files in this folder

| File | Role |
| --- | --- |
| [builder.rs](builder.rs) | Complete bounded task/ordinal/arm matrix construction. |
| [mod.rs](mod.rs) | Deterministic paired rollout planning and D3 work binding. |
| [rollout.rs](rollout.rs) | One immutable logical rollout and its D3 work specification. |
| [seed.rs](seed.rs) | Reproducible paired seed derivation and C5 mapping. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
