# crates/app/peritus-product-runner/src/trace — navigation

## Responsibility

Implementation pieces for `trace.rs`: Durable per-run D0 provider/tool trace.

## Files in this folder

| File | Role |
| --- | --- |
| [accounting.rs](accounting.rs) | Run-owned accounting is updated before another provider or tool boundary can fail. |
| [local_memory.rs](local_memory.rs) | Versioned exact observation/checkpoint records; legacy tags remain unchanged and readable. |
| [reader.rs](reader.rs) | Read-only snapshot iterator; filters frame headers before allocating payloads and supports constant-memory trace inspection. |

Owning module: [trace.rs](../trace.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
