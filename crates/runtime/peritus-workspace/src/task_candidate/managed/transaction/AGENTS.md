# crates/runtime/peritus-workspace/src/task_candidate/managed/transaction — navigation

## Responsibility

Durable explicit discard recovery; inspection never publishes restore effects.

## Files in this folder

| File | Role |
| --- | --- |
| [mod.rs](mod.rs) | Durable explicit discard recovery; inspection never publishes restore effects. |
| [owned.rs](owned.rs) | Nonces distinguish our preparation from foreign Git locks and adjacent user files. |
| [state.rs](state.rs) | Bounded, checksummed restore intent and progress, separate from source effects. |
| [validation.rs](validation.rs) | Validate recorded transitions before retrying any source, index, HEAD, or archive effect. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [validation/](validation/AGENTS.md) | Implementation pieces for `validation.rs`: Validate recorded transitions before retrying any source, index, HEAD, or archive effect. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
