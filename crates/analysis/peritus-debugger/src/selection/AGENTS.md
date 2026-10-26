# crates/analysis/peritus-debugger/src/selection — navigation

## Responsibility

Deterministic immutable C7/C0 evidence selection.

## Files in this folder

| File | Role |
| --- | --- |
| [canonical.rs](canonical.rs) | Canonical schema-v1 selection-manifest encoding. |
| [engine.rs](engine.rs) | Pure bounded selection engine. |
| [manifest.rs](manifest.rs) | Immutable manifest entries and canonical schema-v1 representation. |
| [mod.rs](mod.rs) | Deterministic immutable C7/C0 evidence selection. |
| [provenance.rs](provenance.rs) | Exact C7/C0 and ordinary-artifact provenance checks. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
