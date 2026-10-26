# crates/analysis/peritus-debugger/src/component — navigation

## Responsibility

E1 component correlation and diagnostic-only harness health.

## Files in this folder

| File | Role |
| --- | --- |
| [health.rs](health.rs) | Bounded integer diagnostic health summary without verdict authority. |
| [mapping.rs](mapping.rs) | Taxonomy and trace-pattern mapping to immutable E1 declarations. |
| [mod.rs](mod.rs) | E1 component correlation and diagnostic-only harness health. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
