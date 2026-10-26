# crates/app/peritus-product-runner/src/developer_tools/command_runtime/compactor — navigation

## Responsibility

Implementation pieces for `compactor.rs`: Optional auxiliary inference through the existing C2 authority and native C3 gateway.

## Files in this folder

| File | Role |
| --- | --- |
| [backend.rs](backend.rs) | Select exactly the configured installed native backend; no raw-execution fallback. |
| [sandbox.rs](sandbox.rs) | A cleared-environment, no-network, no-secret contract admitting only inference inputs. |

Owning module: [compactor.rs](../compactor.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
