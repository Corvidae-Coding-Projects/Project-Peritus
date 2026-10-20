# Read reuse navigation

The parent `read_reuse.rs` derives versioned coverage from the existing role-scoped archive. It plans reuse against the exact provider-visible view; workspace tools independently admit permissions and freshness. No receipt grants authority or task-completion credit.

| File | Responsibility |
| --- | --- |
| `page.rs` | Slice actual archived line/byte fragments, preserve UTF-8 boundaries, and fit response metadata plus content. |
| `view.rs` | Credit final visible tool results and exact host-rendered archive previews; omitted/truncated evidence is restored when needed. |

Navigation: [Local context](../AGENTS.md), [parent owner](../read_reuse.rs).
