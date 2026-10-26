# crates/model/peritus-provider-core/src/process — navigation

## Responsibility

Implementation pieces for `process.rs`: Bounded owned subprocess requests with a runtime-private Tokio implementation.

## Files in this folder

| File | Role |
| --- | --- |
| [tokio_transport.rs](tokio_transport.rs) | Tokio-backed subprocess execution, bounded observer-driven protocol stdin, cancellation, deadlines and owned child cleanup. |

Owning module: [process.rs](../process.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
