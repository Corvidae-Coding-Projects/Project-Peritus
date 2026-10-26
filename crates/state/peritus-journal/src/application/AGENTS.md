# crates/state/peritus-journal/src/application — navigation

## Responsibility

Implementation pieces for `application.rs`: Typed durable application ledger and catalog.

## Files in this folder

| File | Role |
| --- | --- |
| [artifact_store.rs](artifact_store.rs) | Transactional application-artifact catalog persistence. |
| [artifact_types.rs](artifact_types.rs) | Checked application-artifact catalog values. |
| [command_store.rs](command_store.rs) | Transactional application-command ledger persistence. |
| [command_types.rs](command_types.rs) | Checked application-command ledger values. |
| [principal_store.rs](principal_store.rs) | Transactional application-principal persistence. |
| [principal_types.rs](principal_types.rs) | Checked application-principal values. |
| [prompt_rows.rs](prompt_rows.rs) | Strict decoding for durable application-prompt rows. |
| [prompt_store.rs](prompt_store.rs) | Transactional application-prompt target and settlement persistence. |
| [prompt_types.rs](prompt_types.rs) | Checked durable application-prompt target and settlement values. |
| [rows.rs](rows.rs) | Strict decoding for application tables. |
| [session_store.rs](session_store.rs) | Transactional application-session persistence. |
| [session_types.rs](session_types.rs) | Checked application-session values. |
| [store.rs](store.rs) | Shared query and error helpers for application-ledger stores. |
| [types.rs](types.rs) | Stable application-ledger type surface. |
| [workspace_store.rs](workspace_store.rs) | Durable application workspace catalog and bounded recovery enumeration. |
| [workspace_types.rs](workspace_types.rs) | Checked application-workspace catalog values. |

Owning module: [application.rs](../application.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
