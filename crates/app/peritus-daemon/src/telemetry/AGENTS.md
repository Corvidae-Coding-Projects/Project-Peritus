# crates/app/peritus-daemon/src/telemetry — navigation

## Responsibility

Implementation pieces for `telemetry.rs`: Redaction-safe bounded local telemetry export.

## Files in this folder

| File | Role |
| --- | --- |
| [local_file.rs](local_file.rs) | Synchronized sequence-named local telemetry batch exporter. |
| [runtime.rs](runtime.rs) | C7 projection recovery, local export, and exact checkpoint ownership. |

Owning module: [telemetry.rs](../telemetry.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
