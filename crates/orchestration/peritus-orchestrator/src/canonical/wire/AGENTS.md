# crates/orchestration/peritus-orchestrator/src/canonical/wire — navigation

## Responsibility

Implementation pieces for `wire.rs`: Shared closed-tag and primitive codec helpers for families 76-78.

## Files in this folder

| File | Role |
| --- | --- |
| [domain.rs](domain.rs) | Canonical codecs for immutable E0 domain records. |
| [observation.rs](observation.rs) | Canonical codecs for checked cross-aggregate observations. |
| [records.rs](records.rs) | Acceptance certificate and terminal fact codecs. |
| [tags.rs](tags.rs) | Explicit closed protocol tags for E0 wire families. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [domain/](domain/AGENTS.md) | Implementation pieces for `domain.rs`: Canonical codecs for immutable E0 domain records. |
| [observation/](observation/AGENTS.md) | Implementation pieces for `observation.rs`: Canonical codecs for checked cross-aggregate observations. |

Owning module: [wire.rs](../wire.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
