# crates/app/peritus-daemon/src/rsi/execution — navigation

## Responsibility

Implementation pieces for `execution.rs`: Credential-free local RSI executions through the committed C2 and native C3 gateway.

## Files in this folder

| File | Role |
| --- | --- |
| [materialize.rs](materialize.rs) | Immutable digest-checked role files, never host paths chosen by a candidate. |
| [recovery.rs](recovery.rs) | Recovery of a proved preparation's empty cgroup; never guessed ownership or broad signals. |
| [sandbox.rs](sandbox.rs) | Private filesystem, cleared environment, credential and network denial, and native limits. |

Owning module: [execution.rs](../execution.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
