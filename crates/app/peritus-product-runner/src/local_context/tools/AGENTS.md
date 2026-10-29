# crates/app/peritus-product-runner/src/local_context/tools — navigation

## Responsibility

Model-facing context read/update operations and schema. These retain evidence/working entries without granting workspace or process authority.

## Files in this folder

| File | Role |
| --- | --- |
| [entry_view.rs](entry_view.rs) | Shared non-authoritative state vocabulary for inspection and optional local inference. |
| [read.rs](read.rs) | Exact byte-range retrieval and bounded literal-search/state pages with scope-bound cursors. |
| [sources.rs](sources.rs) | Byte-bounded readable source descriptors with fixed-snapshot scope-bound cursors and current source-policy checks. |
| [schema.rs](schema.rs) | Strict public JSON schemas mirror bounded parsers, including nested operations. |
| [secret_text.rs](secret_text.rs) | Conservative rejection of recognizable credential material in derived entries. |
| [update.rs](update.rs) | Validate a whole proposal before committing; archive/protocol events do not conflict with it. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [read/](read/AGENTS.md) | Implementation pieces for `read.rs`: Exact byte-range retrieval and bounded literal-search/state pages with scope-bound cursors. |

Owning module: [tools.rs](../tools.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
