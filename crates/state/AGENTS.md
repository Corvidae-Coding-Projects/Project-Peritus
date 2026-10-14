# crates/state — navigation

## Responsibility

Durable persistence and its lifecycle: journal, artifacts, approvals, leases, evidence, migrations, and replayable projections.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [peritus-approval/](peritus-approval/AGENTS.md) | digest-bound human approval transitions for Peritus. |
| [peritus-artifact-store/](peritus-artifact-store/AGENTS.md) | Streaming content-addressed artifact storage for Peritus. |
| [peritus-evidence/](peritus-evidence/AGENTS.md) | Durable provenance, freshness, invalidation, and portable evidence bundles for Peritus. |
| [peritus-journal/](peritus-journal/AGENTS.md) | Transactional durable event journal and commit receipts for Peritus. |
| [peritus-leases/](peritus-leases/AGENTS.md) | generation-bound exclusive mutation leases for Peritus. |
| [peritus-migrations/](peritus-migrations/AGENTS.md) | Forward-only durable schema migrations and recovery for Peritus. |
| [peritus-projection/](peritus-projection/AGENTS.md) | Pure deterministic journal projections and durable shadow rebuilds for Peritus. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
