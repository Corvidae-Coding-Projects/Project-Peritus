# crates/observe/peritus-trace/src/domain — navigation

## Responsibility

Closed content-free trace observation domain.

## Files in this folder

| File | Role |
| --- | --- |
| [attributes.rs](attributes.rs) | Closed typed attribute keys and scalar values. |
| [kinds.rs](kinds.rs) | Closed span, diagnostic, outcome, and status vocabularies. |
| [mod.rs](mod.rs) | Closed content-free trace observation domain. |
| [observation.rs](observation.rs) | Complete bounded observation envelope and canonical collection checks. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
