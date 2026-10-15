# crates/app/peritus-tui/src/render — navigation

## Responsibility

Implementation pieces for `render.rs`: Ratatui rendering for every G2 interaction view.

## Files in this folder

| File | Role |
| --- | --- |
| [chat.rs](chat.rs) | Conversation-first terminal layout with a persistent multiline composer and scrollable text. |
| [editor.rs](editor.rs) | Modal input rendering. |
| [product.rs](product.rs) | Dashboard, diff, and review presentation for daemon-owned coding runs. |
| [prompts.rs](prompts.rs) | Bounded scrolling for complete prompt choices and authority details. |
| [status.rs](status.rs) | Status notices stay visible before optional connection metadata. |
| [tabs.rs](tabs.rs) | Keep the active panel visible when the tab strip is wider than the terminal. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [chat/](chat/AGENTS.md) | Implementation pieces for `chat.rs`: Conversation-first terminal layout with a persistent multiline composer and scrollable text. |
| [product/](product/AGENTS.md) | Implementation pieces for `product.rs`: Dashboard, diff, and review presentation for daemon-owned coding runs. |

Owning module: [render.rs](../render.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
