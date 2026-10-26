# crates/analysis/peritus-debugger/src/citation — navigation

## Responsibility

Manifest-contained source and ordinary-artifact citations.

## Files in this folder

| File | Role |
| --- | --- |
| [artifact.rs](artifact.rs) | Immutable citation values. |
| [mod.rs](mod.rs) | Manifest-contained source and ordinary-artifact citations. |
| [validation.rs](validation.rs) | Canonical citation-set validation. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
