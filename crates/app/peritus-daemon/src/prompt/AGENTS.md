# crates/app/peritus-daemon/src/prompt — navigation

## Responsibility

Implementation pieces for `prompt.rs`: Bounded ownership and authority admission for interactive prompts.

## Files in this folder

| File | Role |
| --- | --- |
| [approval.rs](approval.rs) | Exact B1 challenge binding and signed-decision authentication. |
| [clock.rs](clock.rs) | Epoch-bound monotonic authority time owned by the serialized daemon writer. |
| [error.rs](error.rs) | Redaction-safe prompt-broker failures. |
| [registry.rs](registry.rs) | Bounded exact-correlation prompt registry. |
| [types.rs](types.rs) | Public prompt ownership, admission, and accepted-response values. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [registry/](registry/AGENTS.md) | Implementation pieces for `registry.rs`: Bounded exact-correlation prompt registry. |

Owning module: [prompt.rs](../prompt.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
