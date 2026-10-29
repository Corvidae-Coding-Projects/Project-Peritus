# crates/app/testing/peritus-external-benchmarks/src/terminal_results — navigation

## Responsibility

Implementation pieces for `terminal_results.rs`: Deterministic aggregation of retained Harbor Terminal-Bench results.

## Files in this folder

| File | Role |
| --- | --- |
| [cli.rs](cli.rs) | Strict command grammar for Terminal-Bench campaign reports. |
| [model.rs](model.rs) | Versioned campaign-report model and deterministic aggregation. |
| [parse.rs](parse.rs) | Bounded parsing of Harbor job, trial, native, and pin evidence. |
| [publish.rs](publish.rs) | Atomic, no-overwrite publication for normalized campaign evidence. |
| [validation.rs](validation.rs) | Cross-file invariants for retained campaign evidence. |

Owning module: [terminal_results.rs](../terminal_results.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
