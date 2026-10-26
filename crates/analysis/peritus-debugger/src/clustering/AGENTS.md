# crates/analysis/peritus-debugger/src/clustering — navigation

## Responsibility

Deterministic exact-fingerprint and bounded agglomerative clustering.

## Files in this folder

| File | Role |
| --- | --- |
| [engine.rs](engine.rs) | Input-order-invariant clustering with frozen integer similarity rules. |
| [fingerprint.rs](fingerprint.rs) | Stable pattern fingerprints derived only from typed facts. |
| [mod.rs](mod.rs) | Deterministic exact-fingerprint and bounded agglomerative clustering. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
