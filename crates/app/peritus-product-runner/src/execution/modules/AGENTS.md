# modules — navigation

## Responsibility

Host tool intersection for module stages. The parent modules.rs owns workflow execution.

## Files

| File | Role |
| --- | --- |
| [native.rs](native.rs) | Generic native Rust plugin lifecycle, revision-bound agent/tool broker, live same-module pause/steer/resume handoff, and menu publication. |
| [artifacts.rs](artifacts.rs) | Exact versioned text attachment snapshots, stable user-visible exports, and paged conversation reads. |
| [interaction.rs](interaction.rs) | Prevents a pinned module model role from consuming unadmitted steering; forwards existing provider, accounting and activity boundaries. |
| [tools.rs](tools.rs) | Read-only role allowlists, exact predecessor/resource paging and live permissions. |

Read callers for exact behavior. This map adds no testing or approval gates.

Native runtime details: [native/AGENTS.md](native/AGENTS.md).
## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.
