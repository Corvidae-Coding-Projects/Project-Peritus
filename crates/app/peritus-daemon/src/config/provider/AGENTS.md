# crates/app/peritus-daemon/src/config/provider — navigation

## Responsibility

Implementation pieces for `provider.rs`: Strict immutable C5 provider route declarations.

## Files in this folder

| File | Role |
| --- | --- |
| [locality.rs](locality.rs) | Literal-loopback route admission; DNS and subprocesses are not offline guarantees. |
| [values.rs](values.rs) | Closed capability names, retry policy, and safe configuration diagnostics. |

Owning module: [provider.rs](../provider.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
