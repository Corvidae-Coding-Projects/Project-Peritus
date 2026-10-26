# crates/orchestration/peritus-harness/src — navigation

## Responsibility

Durable checked harness definitions and exact C1 materialization.

## Files in this folder

| File | Role |
| --- | --- |
| [lib.rs](lib.rs) | Durable checked harness definitions and exact C1 materialization. |
| [projection.rs](projection.rs) | Rebuildable read-only harness query projection. |
| [replay.rs](replay.rs) | Deterministic event replay and pending-work recovery classification. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [aggregate/](aggregate/AGENTS.md) | Pure harness command, event, state, and reducer model. |
| [domain/](domain/AGENTS.md) | Pure checked harness graph, immutable revision, and append-only history domain. |
| [durability/](durability/AGENTS.md) | C0 binding, atomic commits, and checked recovery loading. |
| [manifest/](manifest/AGENTS.md) | Strict schema-v1 manifest parsing and C1-backed inventory loading. |
| [materialization/](materialization/AGENTS.md) | Pure exact materialization planning and the narrow C1 executor. |
| [runtime/](runtime/AGENTS.md) | Commit-before-effect runtime driver and effect ports. |
| [wire/](wire/AGENTS.md) | Canonical B3 schema-v1 families 79, 80, and 81. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
