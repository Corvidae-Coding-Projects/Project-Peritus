# Peritus observe guide

## Folder guide

Trace and telemetry projections/exporters; these describe behavior and retained evidence rather than authorizing effects.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [peritus-telemetry/](peritus-telemetry/AGENTS.md) | Bounded OpenTelemetry-compatible projections and exporters for Peritus. |
| [peritus-trace/](peritus-trace/AGENTS.md) | Durable causal tracing and redaction boundary for Peritus. |

