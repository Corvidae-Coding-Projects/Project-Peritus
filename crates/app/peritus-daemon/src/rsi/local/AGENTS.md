# crates/app/peritus-daemon/src/rsi/local — navigation

## Responsibility

Implementation pieces for `local.rs`: Actual restricted local candidate/evaluator port for an already committed E3 attempt.

## Files in this folder

| File | Role |
| --- | --- |
| [catalog.rs](catalog.rs) | Versioned local image manifests: exact programs, supplementary resources, and finite policy. |

Owning module: [local.rs](../local.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
