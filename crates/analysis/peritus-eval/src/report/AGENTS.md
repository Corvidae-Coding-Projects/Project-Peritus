# crates/analysis/peritus-eval/src/report — navigation

## Responsibility

Deterministic analysis and immutable inert evaluation reports.

## Files in this folder

| File | Role |
| --- | --- |
| [analysis.rs](analysis.rs) | Complete ledger-to-statistics analysis. |
| [canonical.rs](canonical.rs) | Canonical analysis and report encoding. |
| [metric.rs](metric.rs) | Report metric values and explicit unavailability reasons. |
| [mod.rs](mod.rs) | Deterministic analysis and immutable inert evaluation reports. |
| [validation.rs](validation.rs) | Immutable report construction and canonical validation. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
