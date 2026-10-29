# crates/orchestration/peritus-agent/src/developer/execution/provider_turn — navigation

## Responsibility

Implementation pieces for `provider_turn.rs`: Bounded provider retries and durable public-text delivery.

## Files in this folder

| File | Role |
| --- | --- |
| [progress.rs](progress.rs) | Observed runtime acceptance/reasoning/generation and phase-preserving wait notices and bounded current-input generation checks around an owned provider future; never simulated model output. |

Owning module: [provider_turn.rs](../provider_turn.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
