# crates/app/peritus-daemon/src/startup — navigation

## Responsibility

Implementation pieces for `startup.rs`: Deterministic production startup composition.

## Files in this folder

| File | Role |
| --- | --- |
| [evolution.rs](evolution.rs) | Config-bound F0 production-pointer recovery and exact E1 revision resolution. |
| [migration.rs](migration.rs) | Forward migration and exact durable-operation reconciliation. |
| [plan.rs](plan.rs) | Pure startup checkpoint sequencing and deterministic kill/restart failpoints. |
| [projection.rs](projection.rs) | Deterministic startup validation and shadow rebuild of every built-in projection. |
| [recovery.rs](recovery.rs) | Durable application and process recovery before intake. |
| [registry.rs](registry.rs) | Strict startup reconciliation of the public approval credential registry. |
| [runtime.rs](runtime.rs) | Live daemon ownership and signal-facing runtime API. |
| [workspace.rs](workspace.rs) | Config-bound C1 registration installation and complete C0 catalog reconciliation. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [plan/](plan/AGENTS.md) | Implementation pieces for `plan.rs`: Pure startup checkpoint sequencing and deterministic kill/restart failpoints. |
| [runtime/](runtime/AGENTS.md) | Implementation pieces for `runtime.rs`: Live daemon ownership and signal-facing runtime API. |

Owning module: [startup.rs](../startup.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
