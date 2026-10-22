# crates/app/peritus-tui/src/model/chat — navigation

## Responsibility

Implementation pieces for `chat.rs`: Persistent conversational composer, explicit slash commands, and provider catalog selection.

## Files in this folder

| File | Role |
| --- | --- |
| [modules.rs](modules.rs) | Local market panel, installed/available selection and generic plugin-supplied actions. |
| [binding.rs](binding.rs) | Reopen exact run destinations without retaining another conversation's input routing. |
| [catalog.rs](catalog.rs) | Typed local command metadata. Only directly entered commands are executable intents. |
| [commands.rs](commands.rs) | Small discoverable command vocabulary; unknown commands never reach the model. |
| [convenience.rs](convenience.rs) | Draft-preserving transcript/file search, clipboard and editor handoffs, copying, and presentation settings. |
| [doctor.rs](doctor.rs) | Focused diagnostic panel; opening or navigating it never starts inference. |
| [history.rs](history.rs) | Scoped durable message recall/search, draft restoration, paging, and stale-response protection; reads never submit inputs. |
| [keys.rs](keys.rs) | Keyboard and deliberate-paste ownership for the persistent composer. |
| [models.rs](models.rs) | Exact provider-discovered selection; manual identifiers are always explicitly marked. |
| [providers.rs](providers.rs) | Configured provider picker and explicit future-turn provider/model replacement on new or retained conversations. |
| [navigation.rs](navigation.rs) | Composer selection and mouse ownership, using the same geometry as rendering. |
| [picker.rs](picker.rs) | Exactly one selection overlay can own conversation input at a time. |
| [workbench.rs](workbench.rs) | Durable control intent tracking; reconnect resolves the original operation, never invents one. |
| [working.rs](working.rs) | One ephemeral busy-period timer, driven by monotonic observations from the UI runtime. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [catalog/](catalog/AGENTS.md) | Implementation pieces for `catalog.rs`: Typed local command metadata. Only directly entered commands are executable intents. |
| [models/](models/AGENTS.md) | Implementation pieces for `models.rs`: Exact provider-discovered selection; manual identifiers are always explicitly marked. |
| [workbench/](workbench/AGENTS.md) | Implementation pieces for `workbench.rs`: Durable control intent tracking; reconnect resolves the original operation, never invents one. |

Owning module: [chat.rs](../chat.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
