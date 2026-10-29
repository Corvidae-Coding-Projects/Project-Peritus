# Provider projection components

## Responsibility

Measure disjoint spans of already rendered provider input. The parent `projection.rs` owns
part hashes, prefix comparisons and per-native-response diagnostics. Measurements carry no
prompt content and do not establish provider token usage or cache acceptance.

| File | Role |
| --- | --- |
| [components.rs](components.rs) | Checked byte/character attribution by host role, block type and fixed marker, borrowing raw encoded JSON spans without another full serialization. |

Trace the Codex and OpenCode request builders before changing span boundaries. Component
totals must partition the measured parts exactly; mixed conversation text remains mixed.

Navigation: [Parent waypoint](../AGENTS.md).
