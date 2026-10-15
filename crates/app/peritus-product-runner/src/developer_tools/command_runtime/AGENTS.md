# crates/app/peritus-product-runner/src/developer_tools/command_runtime — navigation

## Responsibility

Shared C4/C2 command ownership for product developer tools.

## Files in this folder

| File | Role |
| --- | --- |
| [authority.rs](authority.rs) | Exact C4 and C2 committed authority for one product command. |
| [compactor.rs](compactor.rs) | Optional auxiliary inference through the existing C2 authority and native C3 gateway. |
| [computer.rs](computer.rs) | Run-owned persistent computer control through the existing C4/C2 process runtime. |
| [construction.rs](construction.rs) | Construction of managed-workspace and explicitly trusted direct-folder command runtimes. |
| [contract.rs](contract.rs) | Minimal checked acceptance contract for one internally authorized product command. |
| [control.rs](control.rs) | Non-destructive validation and dispatch for active command controls. |
| [dispatch.rs](dispatch.rs) | Starts owned commands through the process/router boundary and projects their observations. |
| [folder_patch.rs](folder_patch.rs) | Exact C0 authority for a separately authenticated registered-folder patch. |
| [identity.rs](identity.rs) | Deterministically derived nominal identities for one product command action. |
| [journal.rs](journal.rs) | Small exact-journal helpers for per-command authority evidence. |
| [kernel.rs](kernel.rs) | Exact C0 lifecycle commitments for one command action. |
| [lease.rs](lease.rs) | Writable workspace lease consumption for one C2 product command. |
| [materialization_authority.rs](materialization_authority.rs) | Exact host-owned C1 authority for an actually committed E1 materialization. |
| [mod.rs](mod.rs) | Shared C4/C2 command ownership for product developer tools. |
| [ordinal.rs](ordinal.rs) | Durable allocation of fresh command identities across runtime instances. |
| [output.rs](output.rs) | Bounded exact output reads scoped to an existing native command receipt. |
| [plan.rs](plan.rs) | C4 call and caller-bound C2 execution-plan construction. |
| [preview.rs](preview.rs) | Typed daemon-facing facade over the existing C4/C2 owned command lifecycle. |
| [preview_backlog.rs](preview_backlog.rs) | An exited preview must settle on the first observation even after unattended output. |
| [preview_terminal.rs](preview_terminal.rs) | A terminal observer keeps the C4 owner alive without moving or duplicating process ownership. |
| [projections.rs](projections.rs) | Bounded durable product projections for reconnect-stable command handles. |
| [recovery.rs](recovery.rs) | Reconciles interrupted command controls and recovered process/artifact events. |
| [restricted_process.rs](restricted_process.rs) | Trusted application-owned restricted children using the ordinary committed C2 authority. |
| [result.rs](result.rs) | Product-facing projection of C4 progress and terminal results. |
| [sandbox.rs](sandbox.rs) | Explicit raw-effect sandbox contract used by the C2 product command plan. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [compactor/](compactor/AGENTS.md) | Implementation pieces for `compactor.rs`: Optional auxiliary inference through the existing C2 authority and native C3 gateway. |
| [preview/](preview/AGENTS.md) | Implementation pieces for `preview.rs`: Typed daemon-facing facade over the existing C4/C2 owned command lifecycle. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
