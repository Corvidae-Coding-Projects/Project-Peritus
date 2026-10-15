# crates/app/peritus-daemon/src/product_run — navigation

## Responsibility

ProductRunService and the durable human workbench: request admission, interaction, persistence, recovery, progress, candidate actions, and improvement inbox execution.

## Files in this folder

| File | Role |
| --- | --- |
| [catalog.rs](catalog.rs) | Provider-advertised catalogs and immutable, explicitly selected role adapters. |
| [construction.rs](construction.rs) | Daemon-owned product service construction and durable-generation rehydration. |
| [deliverable.rs](deliverable.rs) | Explicit accept, commit, export, and discard operations for completed deliverables. |
| [doctor.rs](doctor.rs) | Local diagnostic observations. No provider calls, command execution, repairs, or uploads. |
| [error.rs](error.rs) | Product-run service and configuration failures. |
| [execution.rs](execution.rs) | Active product-run task ownership and terminal projection. |
| [improvements.rs](improvements.rs) | Collect inert suggestions from terminal runs; inference starts only on explicit evaluation. |
| [interaction.rs](interaction.rs) | Conversation semantics, durable input acknowledgements, and public execution activity. |
| [library.rs](library.rs) | Durable local conversation-library projection and literal public-message search. |
| [lifecycle.rs](lifecycle.rs) | Cancellation, restart recovery, and retry lifecycle for product runs. |
| [operation.rs](operation.rs) | One live projection of authoritative run, receipt, and deliverable owners. |
| [permissions.rs](permissions.rs) | Host-intersected permission inspection and admission checks. |
| [persistence.rs](persistence.rs) | Durable product-run snapshots and restart recovery. |
| [preview_evidence.rs](preview_evidence.rs) | Exact command and output bindings retained with preview admission. |
| [progress.rs](progress.rs) | Durable product-run accounting and live user-facing liveness text. |
| [recovery.rs](recovery.rs) | Startup reconciliation for durable terminal candidate handoffs. |
| [request.rs](request.rs) | Daemon-owned execution request, separate from the public application protocol. |
| [snapshot.rs](snapshot.rs) | Product-run snapshot construction and state replacement. |
| [workbench.rs](workbench.rs) | Authenticated A3/domain mapping and the single serialized control-journal owner. |
| [workspace_catalog.rs](workspace_catalog.rs) | Atomically replaced workspace roots, direct-folder trust, and host permissions; validates monotonic immutable catalog updates without changing providers or interrupting runs. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [deliverable/](deliverable/AGENTS.md) | Implementation pieces for `deliverable.rs`: Explicit accept, commit, export, and discard operations for completed deliverables. |
| [execution/](execution/AGENTS.md) | Implementation pieces for `execution.rs`: Active product-run task ownership and terminal projection. |
| [improvements/](improvements/AGENTS.md) | Implementation pieces for `improvements.rs`: Collect inert suggestions from terminal runs; inference starts only on explicit evaluation. |
| [interaction/](interaction/AGENTS.md) | Live conversation/control/provider selection projected into role execution; this is the connection between new user input and an ongoing run. |
| [persistence/](persistence/AGENTS.md) | Implementation pieces for `persistence.rs`: Durable product-run snapshots and restart recovery. |
| [workbench/](workbench/AGENTS.md) | Daemon ownership of workbench conversations, sessions, revisions, review constraints, and UI-facing operations. |

Owning module: [product_run.rs](../product_run.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
