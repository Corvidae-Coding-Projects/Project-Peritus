# crates/state/peritus-journal/src/sqlite — navigation

## Responsibility

Narrow `SQLite` durability adapter.

## Files in this folder

| File | Role |
| --- | --- |
| [append.rs](append.rs) | Atomic checked-plan application. |
| [apply_rows.rs](apply_rows.rs) | Static parameterized row insertion for one checked plan. |
| [authority_store.rs](authority_store.rs) | Authority-clock allocation and current registry observations. |
| [connection.rs](connection.rs) | `SQLite` connection ownership and hardened configuration. |
| [history.rs](history.rs) | Lossless, bounded, content-addressed immutable state history. |
| [mod.rs](mod.rs) | Narrow `SQLite` durability adapter. |
| [observation.rs](observation.rs) | Connection-bound invalidation of reusable aggregate replay. |
| [outbox_store.rs](outbox_store.rs) | Bounded transactional outbox claim and acknowledgement operations. |
| [preconditions.rs](preconditions.rs) | Transaction-local durable precondition observations. |
| [query.rs](query.rs) | Exact immutable query and command-resolution paths. |
| [reader.rs](reader.rs) | Read-only inspection of existing journal state without schema installation or recovery. |
| [schema.rs](schema.rs) | Complete initial release schema. Unshipped development revisions are not migration targets. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [history/](history/AGENTS.md) | Implementation pieces for `history.rs`: Lossless, bounded, content-addressed immutable state history. |
| [query/](query/AGENTS.md) | Implementation pieces for `query.rs`: Exact immutable query and command-resolution paths. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
