# Read reuse navigation

## Responsibility

The parent `read_reuse.rs` derives versioned coverage from the existing role-scoped archive. It plans reuse against the exact provider-visible view; workspace tools independently admit permissions and freshness. No receipt grants authority or task-completion credit.

| File | Responsibility |
| --- | --- |
| [page.rs](page.rs) | Slice actual archived line/byte fragments, preserve UTF-8 boundaries, and fit response metadata plus content. |
| [view.rs](view.rs) | Credit final visible tool results and exact host-rendered archive previews; omitted/truncated evidence is restored when needed. |

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Local context](../AGENTS.md), [parent owner](../read_reuse.rs).
