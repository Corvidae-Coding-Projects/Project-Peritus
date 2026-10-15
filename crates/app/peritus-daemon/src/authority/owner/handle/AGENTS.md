# crates/app/peritus-daemon/src/authority/owner/handle — navigation

## Responsibility

Implementation pieces for `handle.rs`: Cloneable bounded client and lifecycle/storage-neutral requests.

## Files in this folder

| File | Role |
| --- | --- |
| [application.rs](application.rs) | Durable application/session/catalog methods on the bounded authority client. |
| [artifact.rs](artifact.rs) | Artifact transfer methods on the bounded authority client. |
| [orchestrator.rs](orchestrator.rs) | E0 claimed-directive settlement through the sole writable authority owner. |
| [prompt.rs](prompt.rs) | Prompt ownership methods on the bounded authority client. |
| [rsi.rs](rsi.rs) | Bounded messages for genuine native candidate campaign effects. |

Owning module: [handle.rs](../handle.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
