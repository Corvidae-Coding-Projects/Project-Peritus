# crates/app/peritus-tui/src/model — navigation

## Responsibility

UI state and interaction transitions for conversations, workbench views, product controls, review, reconnect, and modal editing.

## Files in this folder

| File | Role |
| --- | --- |
| [chat.rs](chat.rs) | Persistent conversational composer, explicit slash commands, and provider catalog selection. |
| [editor.rs](editor.rs) | Modal editor keyboard handling and typed submission routing. |
| [interaction.rs](interaction.rs) | Keyboard interaction and request construction for mutable UI workflows. |
| [product.rs](product.rs) | Interactive product-run composer and daemon-observation projection. |
| [protocol.rs](protocol.rs) | Protocol message admission and daemon-derived state projection. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [chat/](chat/AGENTS.md) | Implementation pieces for `chat.rs`: Persistent conversational composer, explicit slash commands, and provider catalog selection. |
| [editor/](editor/AGENTS.md) | Implementation pieces for `editor.rs`: Modal editor keyboard handling and typed submission routing. |
| [interaction/](interaction/AGENTS.md) | Implementation pieces for `interaction.rs`: Keyboard interaction and request construction for mutable UI workflows. |
| [product/](product/AGENTS.md) | Implementation pieces for `product.rs`: Interactive product-run composer and daemon-observation projection. |
| [protocol/](protocol/AGENTS.md) | Implementation pieces for `protocol.rs`: Protocol message admission and daemon-derived state projection. |

Owning module: [model.rs](../model.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
