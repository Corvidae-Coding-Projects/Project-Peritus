# crates/orchestration/peritus-orchestrator/src/durability — navigation

## Responsibility

Implementation pieces for `durability.rs`: Atomic C0 persistence, outbox installation, and checked E0 replay loading.

## Files in this folder

| File | Role |
| --- | --- |
| [acknowledgement.rs](acknowledgement.rs) | Checked binding between one published E0 directive and its exact claimed C0 outbox row. |
| [binding.rs](binding.rs) | Cross-record semantic checks before an E0 C0 append. |

Owning module: [durability.rs](../durability.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
