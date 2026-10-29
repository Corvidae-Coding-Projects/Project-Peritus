# crates/orchestration/peritus-gates/src/wire — navigation

## Responsibility

D1-owned typed B3 codecs for reserved families 50, 51, and 52.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Canonical B3 command frame for the D1 gate aggregate. |
| [event.rs](event.rs) | Local implementation for wire. Entry names: `GateEventFrame`, `into_event`, `FAMILY`, `SCHEMA_VERSION`, `encode_payload`. |
| [mod.rs](mod.rs) | D1-owned typed B3 codecs for reserved families 50, 51, and 52. |
| [state.rs](state.rs) | Complete typed C0 checkpoint frame. Its semantic authority remains event replay. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
