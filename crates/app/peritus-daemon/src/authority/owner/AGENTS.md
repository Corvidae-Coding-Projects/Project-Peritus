# crates/app/peritus-daemon/src/authority/owner — navigation

## Responsibility

Implementation pieces for `owner.rs`: Single writable-state authority owner and bounded client facade.

## Files in this folder

| File | Role |
| --- | --- |
| [error.rs](error.rs) | Shared lifecycle, storage, and owner-channel error translation. |
| [handle.rs](handle.rs) | Cloneable bounded client and lifecycle/storage-neutral requests. |
| [orchestrator.rs](orchestrator.rs) | Owner-confined reconstruction and atomic settlement of claimed E0 directives. |
| [prompt.rs](prompt.rs) | Lifecycle and ownership admission for prompt-broker operations. |
| [runtime.rs](runtime.rs) | Serialized receive loop and closed message dispatch. |
| [storage.rs](storage.rs) | Durable command reconciliation retained behind the authority owner. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [handle/](handle/AGENTS.md) | Implementation pieces for `handle.rs`: Cloneable bounded client and lifecycle/storage-neutral requests. |
| [orchestrator/](orchestrator/AGENTS.md) | Implementation pieces for `orchestrator.rs`: Owner-confined reconstruction and atomic settlement of claimed E0 directives. |
| [prompt/](prompt/AGENTS.md) | Implementation pieces for `prompt.rs`: Lifecycle and ownership admission for prompt-broker operations. |

Owning module: [owner.rs](../owner.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
