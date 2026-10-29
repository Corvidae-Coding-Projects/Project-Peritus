# crates/analysis/peritus-evolution/src/change — navigation

## Responsibility

Immutable change manifests, component deltas, predictions, and isolated variants.

## Files in this folder

| File | Role |
| --- | --- |
| [delta.rs](delta.rs) | Exact evolvable E1 component deltas. |
| [manifest.rs](manifest.rs) | Complete immutable evidence-backed change manifests. |
| [mod.rs](mod.rs) | Immutable change manifests, component deltas, predictions, and isolated variants. |
| [prediction.rs](prediction.rs) | Falsifiable typed change predictions. |
| [text.rs](text.rs) | Canonical bounded human-authored text. |
| [variant.rs](variant.rs) | Isolated immutable candidate variants. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
