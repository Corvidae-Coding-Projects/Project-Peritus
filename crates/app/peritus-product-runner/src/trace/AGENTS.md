# crates/app/peritus-product-runner/src/trace — navigation

## Responsibility

Implementation pieces for `trace.rs`: Durable per-run D0 provider/tool trace.

## Files in this folder

| File | Role |
| --- | --- |
| [accounting.rs](accounting.rs) | Run-owned accounting before fallible boundaries, plus content-free attempt latency, actual adapter parts and prefix measurements. |
| [local_memory.rs](local_memory.rs) | Versioned exact observation/checkpoint records; legacy tags remain unchanged and readable. |

Owning module: [trace.rs](../trace.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.

The stable reader now lives in [peritus-trace](../../../../observe/peritus-trace/src/developer/AGENTS.md), reexported by the parent for API compatibility.
