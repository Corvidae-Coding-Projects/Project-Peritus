# crates/orchestration/peritus-orchestrator/src/canonical/wire/observation — navigation

## Responsibility

Implementation pieces for `observation.rs`: Canonical codecs for checked cross-aggregate observations.

## Files in this folder

| File | Role |
| --- | --- |
| [agent.rs](agent.rs) | D0 terminal observation codec. |
| [d3.rs](d3.rs) | D3 activation and terminal observation codecs. |
| [quality.rs](quality.rs) | D1 and D2 quality observation codecs. |

Owning module: [observation.rs](../observation.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
