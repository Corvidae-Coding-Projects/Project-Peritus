# crates/orchestration/peritus-review/src/wire — navigation

## Responsibility

D2-owned typed B3 codecs for reserved families 53, 54, and 55.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Canonical family-53 schema-v1 review command frame. |
| [event.rs](event.rs) | Canonical family-54 schema-v1 review event frame. |
| [mod.rs](mod.rs) | D2-owned typed B3 codecs for reserved families 53, 54, and 55. |
| [state.rs](state.rs) | Canonical family-55 schema-v1 complete review-state checkpoint. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [state/](state/AGENTS.md) | Implementation pieces for `state.rs`: Canonical family-55 schema-v1 complete review-state checkpoint. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
