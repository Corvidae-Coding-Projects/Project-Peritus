# crates/orchestration/peritus-orchestrator/src/qualification — navigation

## Responsibility

Deterministic production-lifecycle fixtures for external resilience qualification.

## Files in this folder

| File | Role |
| --- | --- |
| [certificate.rs](certificate.rs) | Exact B2 certificate construction for the deterministic qualification journey. |
| [domain.rs](domain.rs) | Checked deterministic records used by the lifecycle qualification journey. |
| [journey.rs](journey.rs) | Shortest legal reducer journeys to every H1 daemon lifecycle phase. |
| [mod.rs](mod.rs) | Deterministic production-lifecycle fixtures for external resilience qualification. |
| [scenario.rs](scenario.rs) | Fenced command cursor over the production E0 reducer. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
